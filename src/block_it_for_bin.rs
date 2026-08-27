use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;

use flate2::read::GzDecoder;
use indicatif::ProgressBar;
use zstd::stream::write::Encoder;

use crate::utils::write_hit_trace_bin;

#[derive(Debug, Clone)]
pub struct ProcessedTrace {
    pub block_tags: Vec<u32>,
    pub forward_refs: Vec<i32>,
}

pub fn convert(
    in_file_path: &PathBuf,
    data_path: PathBuf,
    elements_per_block: u32,
    write_hit_trace: bool,
    pb: &ProgressBar,
) -> Result<ProcessedTrace, Box<dyn std::error::Error>> {
    debug_assert!(elements_per_block > 0);
    std::fs::create_dir_all(&data_path)?;

    // --- Single pass: collect hit trace + pc/addr + forward refs together ---
    pb.set_message("reading trace (single pass)…");

    let file = File::open(in_file_path)?;
    let reader: Box<dyn Read> = if in_file_path.extension().is_some_and(|ext| ext == "gz") {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut reader = BufReader::new(reader);
    let mut buffer = [0u8; 9];

    // A native hit trace can be very large, so do not allocate it unless the
    // caller explicitly requested the binary output.
    let mut hit_trace = write_hit_trace.then(Vec::new);
    // Accesses storage: Only (PC, Addr).
    // Clock time is implied by index. Is_hit is gone.
    // 8 bytes per entry vs 16 bytes previously => 50% RAM saving.
    // Keep the complete PC so the phase marker in the top byte survives conversion.
    let mut pc_accesses: Vec<u32> = Vec::new();
    let mut addr_accesses: Vec<u32> = Vec::new();
    let mut forward_ri: Vec<i32> = Vec::new();
    let mut last_seen_index: HashMap<u32, usize> = HashMap::new();

    while reader.read_exact(&mut buffer).is_ok() {
        let pc = u32::from_le_bytes(buffer[0..4].try_into()?);
        let addr = u32::from_le_bytes(buffer[4..8].try_into()?);
        let is_hit = buffer[8] != 0;

        let block_tag = if elements_per_block > 1 {
            addr / elements_per_block
        } else {
            addr
        };
        let curr_idx = addr_accesses.len();

        if let Some(prev_idx) = last_seen_index.insert(block_tag, curr_idx) {
            forward_ri[prev_idx] = (curr_idx as i32) - (prev_idx as i32);
        }

        if let Some(hits) = &mut hit_trace {
            hits.push(is_hit);
        }
        pc_accesses.push(pc);
        addr_accesses.push(addr);
        forward_ri.push(i32::MAX);
    }

    // Drop the forward-ref lookup map to free RAM
    drop(last_seen_index);

    if let Some(hits) = hit_trace {
        // Derive the native trace name from the directory above `traces`
        // (for example, "plru_b512_medium" becomes "hardware_512").
        let hit_trace_type = in_file_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|n| {
                if let Some(idx) = n.find("_b") {
                    let after = &n[idx + 2..];
                    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if !digits.is_empty() {
                        format!("hardware_{}", digits)
                    } else {
                        n.to_string()
                    }
                } else {
                    n.to_string()
                }
            })
            .unwrap_or_else(|| "hardware_000".to_string());

        write_hit_trace_bin(&data_path, &hit_trace_type, &hits)?;
        log::info!("Native hit trace binary output completed");

        // Should i drop the hit trace vector here to free RAM? Or it will be automatically dropped now? I do not want to to be dropped at the end of this function, because it is too late. I want to free the memory before the function ends and specifically now.
        // A: Yes, you can explicitly drop the hit trace vector to free memory before the function ends. You can do this by calling `drop(hits);` after you are done using it. This will free the memory allocated for the hit trace vector immediately, rather than waiting for the function to return and the variable to go out of scope.
        drop(hits);
    }

    // --- Write block trace (zstd-compressed) --------------------------------
    pb.set_message("writing {block/word}_trace.bin.zst…");

    let file_name = if elements_per_block == 1 {
        "block_trace.bin.zst"
    } else {
        "word_trace.bin.zst"
    };
    let output_path = data_path.join(file_name);
    let file = File::create(&output_path)?;
    let buf_writer = BufWriter::new(file);
    let mut writer = Encoder::new(buf_writer, 0)?; // 0 = zstd default level (3)

    for (i, (&pc, &addr)) in pc_accesses.iter().zip(addr_accesses.iter()).enumerate() {
        let mut buffer = [0u8; 12];
        buffer[0..4].copy_from_slice(&pc.to_le_bytes());
        buffer[4..8].copy_from_slice(&forward_ri[i].to_le_bytes());
        buffer[8..12].copy_from_slice(&addr.to_le_bytes());
        writer.write_all(&buffer)?;
    }
    writer.flush()?;
    writer.finish()?;

    drop(pc_accesses);

    // Convert addresses to block tags in-place
    if elements_per_block > 1 {
        for addr in &mut addr_accesses {
            *addr /= elements_per_block;
        }
    }

    Ok(ProcessedTrace {
        block_tags: addr_accesses,
        forward_refs: forward_ri,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_record(bytes: &mut Vec<u8>, pc: u32, address: u32, hit: bool) {
        bytes.extend(pc.to_le_bytes());
        bytes.extend(address.to_le_bytes());
        bytes.push(u8::from(hit));
    }

    #[test]
    fn applies_elements_per_block_and_only_writes_requested_hit_trace() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "constructive-opt-legacy-{}-{unique}",
            std::process::id()
        ));
        let trace_directory = root.join("plru_b8").join("traces");
        std::fs::create_dir_all(&trace_directory).unwrap();
        let trace_path = trace_directory.join("test.bin");

        let mut bytes = Vec::new();
        write_record(&mut bytes, 1, 0, false);
        write_record(&mut bytes, 2, 1, true);
        write_record(&mut bytes, 3, 2, true);
        std::fs::write(&trace_path, bytes).unwrap();

        let without_hits = root.join("without_hits");
        let processed = convert(
            &trace_path,
            without_hits.clone(),
            2,
            false,
            &ProgressBar::hidden(),
        )
        .unwrap();
        assert_eq!(processed.block_tags, vec![0, 0, 1]);
        assert_eq!(processed.forward_refs, vec![1, i32::MAX, i32::MAX]);
        assert!(!without_hits.join("hit_trace_hardware_8.bin").exists());

        let with_hits = root.join("with_hits");
        convert(
            &trace_path,
            with_hits.clone(),
            2,
            true,
            &ProgressBar::hidden(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(with_hits.join("hit_trace_hardware_8.bin")).unwrap(),
            vec![0b0110_0000]
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
