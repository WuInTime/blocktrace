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

    std::fs::create_dir_all(&data_path)?;

    // --- PASS 1: Generate Hit Trace & Count Accesses ---
    {
        let file = File::open(in_file_path)?;
        let reader: Box<dyn Read> = if in_file_path.extension().is_some_and(|ext| ext == "gz") {
            Box::new(GzDecoder::new(file))
        } else {
            Box::new(file)
        };
        let mut reader = BufReader::new(reader);
        let mut buffer = [0u8; 9];
        let mut hit_trace: Vec<bool> = Vec::new();

        while reader.read_exact(&mut buffer).is_ok() {
            let is_hit = buffer[8] != 0;
            hit_trace.push(is_hit);
        }

        write_hit_trace_bin(&data_path, "plru_512", &hit_trace)?;
        log::info!("Hit Trace Binary Output Completed. RAM freed.");
        // hit_trace is dropped here, freeing ~1GB (for 1B entries)
    }

    // --- PASS 2: Generate RI & Output Trace ---
    let file = File::open(in_file_path)?;
    let reader: Box<dyn Read> = if in_file_path.extension().is_some_and(|ext| ext == "gz") {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut reader = BufReader::new(reader);
    let mut buffer = [0u8; 9];

    // Accesses storage: Only (PC, Addr).
    // Clock time is implied by index. Is_hit is gone.
    // 8 bytes per entry vs 16 bytes previously => 50% RAM saving.
    let mut accesses: Vec<(u32, u32)> = Vec::new();
    let mut block_access_indices: HashMap<u32, Vec<usize>> = HashMap::new();

    while reader.read_exact(&mut buffer).is_ok() {
        let pc = u32::from_le_bytes(buffer[0..4].try_into()?);
        let addr = u32::from_le_bytes(buffer[4..8].try_into()?);
        // we ignore is_hit in this pass

        let block_tag = addr / BLOCK_SIZE;

        block_access_indices
            .entry(block_tag)
            .or_default()
            .push(accesses.len()); // store index

        accesses.push((pc, addr));
    }

    let mut forward_ri: Vec<i32> = vec![i32::MAX; accesses.len()];

    // Calculate Forward RI
    for indices in block_access_indices.values() {
        for w in indices.windows(2) {
            let curr = w[0];
            let next = w[1];
            // Time difference is simply index difference because time increments by 1 per access
            // Original code: accesses[next].clock - accesses[curr].clock
            // Since clock starts at 1 and increments by 1: (next+1) - (curr+1) = next - curr
            let dt = (next as i32) - (curr as i32);
            forward_ri[curr] = dt;
        }
    }

    // Drop the map to free ~8GB+ RAM (for 1B entries) before writing output if possible?
    // We need both 'accesses' and 'forward_ri' for writing.
    drop(block_access_indices);

    // Write Final Output (zstd-compressed)
    let file_name = if BLOCK_SIZE == 1 {
        "block_trace.bin.zst"
    } else {
        "word_trace.bin.zst"
    };
    let output_path = data_path.join(file_name);
    let file = File::create(&output_path)?;
    let buf_writer = BufWriter::new(file);
    let mut writer = Encoder::new(buf_writer, 0)?; // 0 = zstd default level which is 3

    let mut entries: Vec<TraceEntry> = Vec::with_capacity(accesses.len());

    for (i, (pc, addr)) in accesses.iter().enumerate() {
        writer.write_all(&pc.to_le_bytes())?;
        writer.write_all(&forward_ri[i].to_le_bytes())?;
        writer.write_all(&addr.to_le_bytes())?;

        entries.push(TraceEntry {
            block_tag: addr / BLOCK_SIZE,
            forward_ri: forward_ri[i],
        });
    }
    writer.flush()?;
    writer.finish()?; // Finalize zstd stream

    log::info!("Block Trace Binary Output Completed.");

    Ok(entries)
}
