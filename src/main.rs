#![allow(dead_code)]
#![allow(unused_imports)]
use constructive_opt::{block_it, block_it_forward, opt_miss_ratio, utils::*};
use lru_sim::update_hit_miss_csv;
use serde::Deserialize;
use std::error::Error;
use std::path::{Path, PathBuf};
use utils;

#[derive(Debug, Deserialize)]
struct RawAccessTrace {
    address: String,
}

fn generate_opt_miss_ratio_data(
    trace: &[usize],
    max_cache_size: usize,
    data_path: &Path,
    count_cold_as_hit: bool,
) -> Result<(), Box<dyn Error>> {
    let mut cache_size = 64;
    while cache_size <= max_cache_size {
        let miss_result = opt_miss_ratio(trace, cache_size, count_cold_as_hit);

        let hit_trace: Vec<u8> = miss_result
            .hit_trace
            .iter()
            .map(|&is_hit| if is_hit { 1 } else { 0 })
            .collect();
        // let miss_count = hit_trace.iter().filter(|&&x| x == 0).count();
        // println!("opt_{}: total misses = {}", cache_size, miss_count);

        let col_name = format!("opt_{}", cache_size);
        update_hit_miss_csv(data_path, &col_name, &hit_trace)?;

        cache_size = calculate_next_cache_size(cache_size, true);
    }
    println!();
    Ok(())
}

fn blockit_and_opt_miss_ratio(data_path: PathBuf, count_cold_as_hit: bool) {
    // let in_file_path = format!("{}{}", data_path, "/memtrace.csv");
    let out_directory = data_path.parent().unwrap().join("results");
    // println!("out_directory: {}", out_directory.to_str().unwrap());
    let bench_result_dir = out_directory.join(data_path.file_stem().unwrap());
    let trace = match block_it_forward::convert(data_path, bench_result_dir.clone()) {
        Ok(trace) => trace,
        Err(_e) => {
            // eprintln!("Error: {}", _e);
            return;
        }
    };
    // let trace = match block_it::convert("/memtrace.csv", &data_path) {
    //     Ok(trace) => trace,
    //     Err(_e) => {
    //         // eprintln!("Error: {}", _e);
    //         return;
    //     }
    // };
    generate_opt_miss_ratio_data(&trace, 128, &bench_result_dir, count_cold_as_hit).unwrap();
}

pub fn main() {
    let count_cold_as_hit = false;

    // let data_path = "./out/clam/rit/trace_64/heat-3d.csv";
    // blockit_and_opt_miss_ratio(data_path.into(), count_cold_as_hit);

    let csv_files = utils::get_csv_files("./out/clam/rit/trace_64");
    // println!("csv_files: {:?}", csv_files);
    for csv_file in &csv_files {
        blockit_and_opt_miss_ratio(csv_file.clone(), count_cold_as_hit);
    }

    // let subdirectories = utils::get_subdirectories("./out/clam/mvt-part");
    // for subdir in subdirectories {
    //     let data_path = subdir;
    //     blockit_and_opt_miss_ratio(&data_path, count_cold_as_hit);
    // }
}
