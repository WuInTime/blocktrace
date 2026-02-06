use bitvec::prelude::*;
use csv::ReaderBuilder;
use indicatif::ProgressBar;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

pub fn read_third_column_as_usize_vec(file_path: &str) -> Result<Vec<usize>, Box<dyn Error>> {
    let file = File::open(file_path)?;
    let mut rdr = ReaderBuilder::new().has_headers(true).from_reader(file);
    let mut trace = Vec::new();

    for result in rdr.records() {
        let record = result?;
        let value: usize = usize::from_str_radix(&record[2], 16)?;
        trace.push(value);
    }

    Ok(trace)
}

#[allow(dead_code)]
pub fn calculate_next_cache_size(cache_size: usize, double_up: bool) -> usize {
    if double_up {
        cache_size * 2
    } else {
        if cache_size == 1 {
            2
        } else if cache_size < 34 {
            cache_size + 2
        } else {
            let mut target = (cache_size * 12 + 5) / 10; // Equivalent to rounding cache_size * 1.2
            if target % 2 != 0 {
                target += 1; // Ensure target is even
            }
            let next_power_of_two = (cache_size + 1).next_power_of_two();
            if target < next_power_of_two {
                target
            } else {
                next_power_of_two
            }
        }
    }
}

pub fn create_progress_bar(max_cache_size: usize) -> ProgressBar {
    let bar = ProgressBar::new(max_cache_size as u64);
    bar.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:60.cyan/blue} {percent}% {pos:>7}/{len:7} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );
    bar
}

pub fn write_hit_trace_bin(dir: &Path, file_name: &str, hits: &[bool]) -> std::io::Result<()> {
    let path = dir.join(format!("hit_trace_{}.bin", file_name));
    let mut writer = BufWriter::new(File::create(path)?);

    // Build and write MSB-first packed bytes (zero-copy)
    let bits: BitVec<u8, Msb0> = hits.iter().copied().collect();
    let bytes = (bits.len() + 7) / 8;
    writer.write_all(&bits.as_raw_slice()[..bytes])?;
    Ok(())
}


pub fn get_files_with_extension(directory: impl AsRef<Path>, ext: &str) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(directory)
        .into_iter()
        .flatten() // Handles the Result from read_dir
        .filter_map(Result::ok) // Filter out any failed entry reads
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file() && path.extension().map_or(false, |s| s == ext)
        })
        .collect();

    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    files
}