use crate::{BlockTrace, NO_FORWARD_REFERENCE, read_record};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;
use xz2::read::XzDecoder;

const RECORD_SIZE: usize = 64;
const CACHE_LINE_SIZE: u64 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryAccess {
    pub pc: u64,
    pub address: u64,
    pub kind: AccessKind,
}

fn u64_at(record: &[u8; RECORD_SIZE], offset: usize) -> u64 {
    u64::from_le_bytes(record[offset..offset + 8].try_into().unwrap())
}

pub fn read_accesses(
    mut reader: impl Read,
    mut emit: impl FnMut(MemoryAccess) -> io::Result<()>,
) -> io::Result<()> {
    let mut record = [0; RECORD_SIZE];
    let mut index = 0;

    loop {
        let bytes_read = read_record(&mut reader, &mut record)?;
        if bytes_read == 0 {
            return Ok(());
        }
        if bytes_read != RECORD_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "incomplete ChampSim record at index {index}: expected {RECORD_SIZE} bytes, got {bytes_read}"
                ),
            ));
        }

        let pc = u64_at(&record, 0);
        // Reads precede writes for read-modify-write instructions. Slot order
        // within each operand group is preserved.
        for offset in (32..64).step_by(8) {
            let address = u64_at(&record, offset);
            if address != 0 {
                emit(MemoryAccess {
                    pc,
                    address,
                    kind: AccessKind::Read,
                })?;
            }
        }
        for offset in (16..32).step_by(8) {
            let address = u64_at(&record, offset);
            if address != 0 {
                emit(MemoryAccess {
                    pc,
                    address,
                    kind: AccessKind::Write,
                })?;
            }
        }
        index += 1;
    }
}

fn trace_reader(path: &Path) -> io::Result<Box<dyn Read>> {
    let file = File::open(path)?;
    if path.extension().is_some_and(|extension| extension == "xz") {
        Ok(Box::new(XzDecoder::new(file)))
    } else {
        Ok(Box::new(file))
    }
}

pub fn read_trace(path: &Path, emit: impl FnMut(MemoryAccess) -> io::Result<()>) -> io::Result<()> {
    read_accesses(BufReader::new(trace_reader(path)?), emit)
}

pub fn convert(
    input_path: &Path,
    output_directory: &Path,
) -> Result<BlockTrace, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output_directory)?;
    let mut trace = BlockTrace {
        pcs: Vec::new(),
        block_tags: Vec::new(),
        forward_refs: Vec::new(),
    };
    let mut last_seen = HashMap::<u32, usize>::new();

    read_trace(input_path, |access| {
        let pc = u32::try_from(access.pc).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "ChampSim PC does not fit in u32",
            )
        })?;
        let block_tag = u32::try_from(access.address / CACHE_LINE_SIZE).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "ChampSim cache-line address does not fit in u32",
            )
        })?;
        let current = trace.len();
        if let Some(previous) = last_seen.insert(block_tag, current) {
            trace.forward_refs[previous] = i32::try_from(current - previous).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "forward reuse interval exceeds i32",
                )
            })?;
        }
        trace.pcs.push(pc);
        trace.block_tags.push(block_tag);
        trace.forward_refs.push(NO_FORWARD_REFERENCE);
        Ok(())
    })?;

    trace.write_zstd(output_directory.join("block_trace.bin.zst"))?;
    Ok(trace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use std::time::{SystemTime, UNIX_EPOCH};
    use xz2::write::XzEncoder;

    fn record(pc: u64, destinations: &[u64], sources: &[u64]) -> [u8; RECORD_SIZE] {
        let mut record = [0; RECORD_SIZE];
        record[0..8].copy_from_slice(&pc.to_le_bytes());
        for (slot, address) in destinations.iter().enumerate() {
            record[16 + slot * 8..24 + slot * 8].copy_from_slice(&address.to_le_bytes());
        }
        for (slot, address) in sources.iter().enumerate() {
            record[32 + slot * 8..40 + slot * 8].copy_from_slice(&address.to_le_bytes());
        }
        record
    }

    #[test]
    fn skips_register_only_and_preserves_operand_order() {
        let mut bytes = record(1, &[], &[]).to_vec();
        bytes.extend(record(2, &[0x30, 0x40], &[0x10, 0, 0x20]));
        let mut accesses = Vec::new();
        read_accesses(Cursor::new(bytes), |access| {
            accesses.push(access);
            Ok(())
        })
        .unwrap();

        assert_eq!(
            accesses,
            vec![
                MemoryAccess {
                    pc: 2,
                    address: 0x10,
                    kind: AccessKind::Read
                },
                MemoryAccess {
                    pc: 2,
                    address: 0x20,
                    kind: AccessKind::Read
                },
                MemoryAccess {
                    pc: 2,
                    address: 0x30,
                    kind: AccessKind::Write
                },
                MemoryAccess {
                    pc: 2,
                    address: 0x40,
                    kind: AccessKind::Write
                },
            ]
        );
    }

    #[test]
    fn rejects_incomplete_records() {
        let error = read_accesses(Cursor::new(vec![0; RECORD_SIZE - 1]), |_| Ok(())).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("index 0"));
    }

    #[test]
    fn reads_raw_and_xz_files() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "blocktrace-champsim-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();

        let contents = record(7, &[0x80], &[0x40]);
        let raw_path = directory.join("test.champsimtrace");
        std::fs::write(&raw_path, contents).unwrap();

        let xz_path = directory.join("test.champsimtrace.xz");
        let mut encoder = XzEncoder::new(File::create(&xz_path).unwrap(), 1);
        encoder.write_all(&contents).unwrap();
        encoder.finish().unwrap();

        for path in [raw_path, xz_path] {
            let mut accesses = Vec::new();
            read_trace(&path, |access| {
                accesses.push(access);
                Ok(())
            })
            .unwrap();
            assert_eq!(accesses.len(), 2);
        }

        std::fs::remove_dir_all(directory).unwrap();
    }
}
