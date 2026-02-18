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
pub struct ProcessedTrace {
    pub block_tags: Vec<u32>,
    pub forward_refs: Vec<i32>,
}

pub fn convert(
    in_file_path: &PathBuf,
    data_path: PathBuf,
) -> Result<ProcessedTrace, Box<dyn std::error::Error>> {
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
        // hit_trace is dropped here, (freeing ~1GB for 1B entries)
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
    // Optimization: separate vectors for u16 PC and u32 Addr saves 2 bytes per entry (6 bytes vs 8 bytes).
    let mut pc_accesses: Vec<u16> = Vec::new();
    let mut addr_accesses: Vec<u32> = Vec::new();
    let mut forward_ri: Vec<i32> = Vec::new();
    let mut last_seen_index: HashMap<u32, usize> = HashMap::new();

    while reader.read_exact(&mut buffer).is_ok() {
        let pc = u32::from_le_bytes(buffer[0..4].try_into()?);
        let addr = u32::from_le_bytes(buffer[4..8].try_into()?);
        // we ignore is_hit in this pass

        let block_tag = if BLOCK_SIZE > 1 {
            addr / BLOCK_SIZE
        } else {
            addr
        };
        let curr_idx: usize = addr_accesses.len();

        if let Some(prev_idx) = last_seen_index.insert(block_tag, curr_idx) {
            let dist = (curr_idx as i32) - (prev_idx as i32);
            forward_ri[prev_idx] = dist;
        }

        pc_accesses.push(pc as u16);
        addr_accesses.push(addr);
        forward_ri.push(i32::MAX);
    }

    // Drop the map to free RAM
    drop(last_seen_index);

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

    for (i, (&pc, &addr)) in pc_accesses.iter().zip(addr_accesses.iter()).enumerate() {
        let mut buffer = [0u8; 12];
        buffer[0..4].copy_from_slice(&(pc as u32).to_le_bytes());
        buffer[4..8].copy_from_slice(&forward_ri[i].to_le_bytes());
        buffer[8..12].copy_from_slice(&addr.to_le_bytes());
        writer.write_all(&buffer)?;
    }
    writer.flush()?;
    writer.finish()?; // Finalize zstd stream

    // Free PC vector memory
    drop(pc_accesses);

    println!("Block Trace Binary Output Completed.");

    // Convert addresses to block tags in-place
    if BLOCK_SIZE > 1 {
        for addr in &mut addr_accesses {
            *addr /= BLOCK_SIZE;
        }
    }

    Ok(ProcessedTrace {
        block_tags: addr_accesses,
        forward_refs: forward_ri,
    })
}
