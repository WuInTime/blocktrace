use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;

use flate2::read::GzDecoder;
use zstd::stream::write::Encoder;

use lru_sim::write_hit_trace_bin;

// const BLOCK_SIZE: u32 = 16;
const BLOCK_SIZE: u32 = 1;

#[derive(Debug, Clone)]
pub struct TraceEntry {
    pub block_tag: u32,
    pub forward_ri: i32,
}

pub fn convert(
    in_file_path: &PathBuf,
    data_path: PathBuf,
) -> Result<Vec<TraceEntry>, Box<dyn std::error::Error>> {
    println!("trace_path: {:?}", in_file_path);

    let file = File::open(in_file_path)?;
    let reader: Box<dyn Read> = if in_file_path.extension().is_some_and(|ext| ext == "gz") {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut file = BufReader::new(reader);
    let mut buffer = [0u8; 9];

    // For forward RI: group by block_tag (not word address!)
    let mut block_access_indices: HashMap<u32, Vec<usize>> = HashMap::new();
    let mut accesses: Vec<(u32, u32, i32, bool)> = Vec::new(); // (pc, addr, clock_time, is_hit)
    let mut clock_time = 1;
    let mut hit_trace: Vec<bool> = Vec::new();

    while file.read_exact(&mut buffer).is_ok() {
        let pc = u32::from_le_bytes(buffer[0..4].try_into()?);
        let addr = u32::from_le_bytes(buffer[4..8].try_into()?);
        let is_hit = buffer[8] != 0;

        let block_tag = addr / BLOCK_SIZE; // used only for RI calculation

        hit_trace.push(is_hit);
        accesses.push((pc, addr, clock_time, is_hit));
        block_access_indices
            .entry(block_tag)
            .or_default()
            .push(accesses.len() - 1);
        clock_time += 1;
    }

    let mut forward_ri: Vec<i32> = vec![i32::MAX; accesses.len()];
    for indices in block_access_indices.values() {
        for w in indices.windows(2) {
            let curr = w[0];
            let next = w[1];
            let dt = accesses[next].2 - accesses[curr].2;
            forward_ri[curr] = dt;
        }
    }

    let mut entries: Vec<TraceEntry> = Vec::with_capacity(accesses.len());
    std::fs::create_dir_all(&data_path)?;

    // 1. Write trace (zstd-compressed, 12 bytes per entry)
    let file_name = if BLOCK_SIZE == 1 {
        "block_trace.bin.zst"
    } else {
        "word_trace.bin.zst"
    };
    let output_path = data_path.join(file_name);
    let file = File::create(&output_path)?;
    let buf_writer = BufWriter::new(file);
    let mut writer = Encoder::new(buf_writer, 0)?; // 0 = zstd default level which is 3

    for (i, (inst_ptr, word_addr, _clock_time, _is_hit)) in accesses.iter().enumerate() {
        writer.write_all(&inst_ptr.to_le_bytes())?;
        writer.write_all(&forward_ri[i].to_le_bytes())?;
        writer.write_all(&word_addr.to_le_bytes())?;

        entries.push(TraceEntry {
            block_tag: word_addr / BLOCK_SIZE,
            forward_ri: forward_ri[i],
        });
    }
    writer.flush()?;
    writer.finish()?; // Finalize zstd stream

    write_hit_trace_bin(&data_path, "plru_512", &hit_trace)?;

    log::info!("Block Trace and Hit Trace Binary Output Completed.");

    Ok(entries)
}
