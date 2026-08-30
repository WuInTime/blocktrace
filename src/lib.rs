//! Memory-trace normalization for cache and lease simulators.

pub mod champsim;
mod discovery;
pub mod legacy;

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::num::NonZeroU32;
use std::path::Path;
use zstd::stream::{read::Decoder, write::Encoder};

pub use discovery::{DiscoveredTrace, TraceFormat, detect_trace_format, discover_traces};

/// Sentinel used when an access has no later reference to the same block.
pub const NO_FORWARD_REFERENCE: i32 = i32::MAX;

/// Canonical in-memory representation of a block trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTrace {
    pub pcs: Vec<u32>,
    pub block_tags: Vec<u32>,
    pub forward_refs: Vec<i32>,
}

impl BlockTrace {
    pub fn len(&self) -> usize {
        self.block_tags.len()
    }

    pub fn is_empty(&self) -> bool {
        self.block_tags.is_empty()
    }

    fn validate(&self) -> io::Result<()> {
        if self.pcs.len() != self.block_tags.len()
            || self.forward_refs.len() != self.block_tags.len()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "PC, block-tag, and forward-reference arrays must have equal lengths",
            ));
        }
        Ok(())
    }

    /// Write the canonical 12-byte record format through zstd compression.
    pub fn write_zstd(&self, path: impl AsRef<Path>) -> io::Result<()> {
        self.validate()?;
        let output = BufWriter::new(File::create(path)?);
        let mut writer = Encoder::new(output, 0)?;
        for ((pc, forward_ref), block_tag) in self
            .pcs
            .iter()
            .zip(&self.forward_refs)
            .zip(&self.block_tags)
        {
            writer.write_all(&pc.to_le_bytes())?;
            writer.write_all(&forward_ref.to_le_bytes())?;
            writer.write_all(&block_tag.to_le_bytes())?;
        }
        writer.finish()?;
        Ok(())
    }

    /// Read the canonical zstd-compressed 12-byte record format.
    pub fn read_zstd(path: impl AsRef<Path>) -> io::Result<Self> {
        let input = BufReader::new(File::open(path)?);
        let mut reader = Decoder::new(input)?;
        let mut trace = Self {
            pcs: Vec::new(),
            block_tags: Vec::new(),
            forward_refs: Vec::new(),
        };
        let mut record = [0_u8; 12];

        loop {
            let bytes_read = read_record(&mut reader, &mut record)?;
            if bytes_read == 0 {
                break;
            }
            if bytes_read != record.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "incomplete block-trace record: expected {} bytes, got {bytes_read}",
                        record.len()
                    ),
                ));
            }
            trace
                .pcs
                .push(u32::from_le_bytes(record[0..4].try_into().unwrap()));
            trace
                .forward_refs
                .push(i32::from_le_bytes(record[4..8].try_into().unwrap()));
            trace
                .block_tags
                .push(u32::from_le_bytes(record[8..12].try_into().unwrap()));
        }
        Ok(trace)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConvertOptions {
    /// Number of input-address elements represented by one cache block.
    pub elements_per_block: NonZeroU32,
    /// Preserve the hit bits embedded in the legacy input trace.
    pub write_native_hit_trace: bool,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            elements_per_block: NonZeroU32::MIN,
            write_native_hit_trace: false,
        }
    }
}

/// Convert a supported input trace and write `block_trace.bin.zst`.
pub fn convert(
    input_path: impl AsRef<Path>,
    output_directory: impl AsRef<Path>,
    format: TraceFormat,
    options: ConvertOptions,
) -> Result<BlockTrace, Box<dyn std::error::Error>> {
    let input_path = input_path.as_ref();
    let output_directory = output_directory.as_ref();
    match format {
        TraceFormat::LegacyBinary => legacy::convert(
            input_path,
            output_directory,
            options.elements_per_block,
            options.write_native_hit_trace,
        ),
        TraceFormat::ChampSim => champsim::convert(input_path, output_directory),
    }
}

fn read_record(reader: &mut impl Read, record: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < record.len() {
        match reader.read(&mut record[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn block_trace_round_trip() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "blocktrace-round-trip-{}-{unique}.bin.zst",
            std::process::id()
        ));
        let expected = BlockTrace {
            pcs: vec![1, 2, 3],
            block_tags: vec![7, 8, 7],
            forward_refs: vec![2, NO_FORWARD_REFERENCE, NO_FORWARD_REFERENCE],
        };
        expected.write_zstd(&path).unwrap();
        assert_eq!(BlockTrace::read_zstd(&path).unwrap(), expected);
        std::fs::remove_file(path).unwrap();
    }
}
