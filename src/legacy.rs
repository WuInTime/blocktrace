use crate::{BlockTrace, NO_FORWARD_REFERENCE, read_record};
use bitvec::prelude::*;
use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::num::NonZeroU32;
use std::path::Path;

pub fn convert(
    input_path: &Path,
    output_directory: &Path,
    elements_per_block: NonZeroU32,
    write_native_hit_trace: bool,
) -> Result<BlockTrace, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output_directory)?;

    let file = File::open(input_path)?;
    let input: Box<dyn Read> = if input_path.extension().is_some_and(|ext| ext == "gz") {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut reader = BufReader::new(input);
    let mut record = [0_u8; 9];
    let mut native_hits = write_native_hit_trace.then(Vec::new);
    let mut trace = BlockTrace {
        pcs: Vec::new(),
        block_tags: Vec::new(),
        forward_refs: Vec::new(),
    };
    let mut last_seen = HashMap::<u32, usize>::new();

    loop {
        let bytes_read = read_record(&mut reader, &mut record)?;
        if bytes_read == 0 {
            break;
        }
        if bytes_read != record.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "incomplete legacy record at index {}: expected {} bytes, got {bytes_read}",
                    trace.len(),
                    record.len()
                ),
            )
            .into());
        }

        let pc = u32::from_le_bytes(record[0..4].try_into().unwrap());
        let address = u32::from_le_bytes(record[4..8].try_into().unwrap());
        let block_tag = address / elements_per_block.get();
        let current = trace.len();
        if let Some(previous) = last_seen.insert(block_tag, current) {
            trace.forward_refs[previous] = i32::try_from(current - previous).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "forward reuse interval exceeds i32",
                )
            })?;
        }
        if let Some(hits) = &mut native_hits {
            hits.push(record[8] != 0);
        }
        trace.pcs.push(pc);
        trace.block_tags.push(block_tag);
        trace.forward_refs.push(NO_FORWARD_REFERENCE);
    }

    if let Some(hits) = native_hits {
        write_hit_trace(output_directory, &native_hit_trace_name(input_path), &hits)?;
    }
    trace.write_zstd(output_directory.join("block_trace.bin.zst"))?;
    Ok(trace)
}

fn native_hit_trace_name(input_path: &Path) -> String {
    input_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(|name| {
            if let Some(index) = name.find("_b") {
                let digits: String = name[index + 2..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                if !digits.is_empty() {
                    return format!("hardware_{digits}");
                }
            }
            name.to_owned()
        })
        .unwrap_or_else(|| "hardware_000".to_owned())
}

fn write_hit_trace(directory: &Path, name: &str, hits: &[bool]) -> io::Result<()> {
    let path = directory.join(format!("hit_trace_{name}.bin"));
    let mut writer = BufWriter::new(File::create(path)?);
    let bits: BitVec<u8, Msb0> = hits.iter().copied().collect();
    let bytes = bits.len().div_ceil(8);
    writer.write_all(&bits.as_raw_slice()[..bytes])
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

    fn temp_root(test_name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "blocktrace-{test_name}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn normalizes_addresses_and_only_writes_requested_hit_trace() {
        let root = temp_root("legacy");
        let trace_directory = root.join("plru_b8").join("traces");
        std::fs::create_dir_all(&trace_directory).unwrap();
        let trace_path = trace_directory.join("test.bin");

        let mut bytes = Vec::new();
        write_record(&mut bytes, 1, 0, false);
        write_record(&mut bytes, 2, 1, true);
        write_record(&mut bytes, 3, 2, true);
        std::fs::write(&trace_path, bytes).unwrap();

        let without_hits = root.join("without_hits");
        let trace = convert(
            &trace_path,
            &without_hits,
            NonZeroU32::new(2).unwrap(),
            false,
        )
        .unwrap();
        assert_eq!(trace.pcs, vec![1, 2, 3]);
        assert_eq!(trace.block_tags, vec![0, 0, 1]);
        assert_eq!(
            trace.forward_refs,
            vec![1, NO_FORWARD_REFERENCE, NO_FORWARD_REFERENCE]
        );
        assert_eq!(
            BlockTrace::read_zstd(without_hits.join("block_trace.bin.zst")).unwrap(),
            trace
        );
        assert!(!without_hits.join("hit_trace_hardware_8.bin").exists());

        let with_hits = root.join("with_hits");
        convert(&trace_path, &with_hits, NonZeroU32::new(2).unwrap(), true).unwrap();
        assert_eq!(
            std::fs::read(with_hits.join("hit_trace_hardware_8.bin")).unwrap(),
            vec![0b0110_0000]
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_incomplete_records() {
        let root = temp_root("incomplete-legacy");
        std::fs::create_dir_all(&root).unwrap();
        let input = root.join("bad.bin");
        std::fs::write(&input, [0_u8; 8]).unwrap();
        let error = convert(&input, &root.join("out"), NonZeroU32::MIN, false).unwrap_err();
        assert!(error.to_string().contains("incomplete legacy record"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
