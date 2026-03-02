use constructive_opt::block_it_for_bin::{self, ProcessedTrace};
use constructive_opt::opt_miss_ratio;
use constructive_opt::utils::*;
use rayon::prelude::*;
use std::error::Error;
use std::path::{Path, PathBuf};

fn generate_opt_miss_ratio_data(
    trace: &ProcessedTrace,
    data_path: &Path,
    count_cold_as_hit: bool,
) -> Result<(), Box<dyn Error>> {
    // Generate OPT miss ratio data for cache sizes: 128 and 512
    let cache_sizes = [128, 512];

    for &cache_size in &cache_sizes {
        let miss_result = opt_miss_ratio(
            &trace.block_tags,
            &trace.forward_refs,
            cache_size,
            count_cold_as_hit,
        );
        let col_name = format!("OPT_{}", cache_size);
        write_hit_trace_bin(data_path, &col_name, &miss_result.hit_trace)?;
    }

    println!();

    Ok(())
}

fn blockit_and_opt_miss_ratio(
    data_path: PathBuf,
    count_cold_as_hit: bool,
) -> Result<(), Box<dyn Error>> {
    let out_directory = data_path
        .parent()
        .ok_or("Invalid data path parent")?
        .parent()
        .ok_or("Invalid data path grand-parent")?
        .join("results");

    let bench_result_dir = out_directory.join(data_path.file_stem().ok_or("Invalid file stem")?);

    // We log errors inside here or propagate them?
    // The original code printed error and returned.
    // Let's propagate error for cleaner handling.
    let trace = block_it_for_bin::convert(&data_path, bench_result_dir.clone())?;

    generate_opt_miss_ratio_data(&trace, &bench_result_dir, count_cold_as_hit)?;
    Ok(())
}

pub fn main() {
    env_logger::init(); // Initialize logger

    let physical_cores = num_cpus::get_physical();
    rayon::ThreadPoolBuilder::new()
        .num_threads(physical_cores)
        .build_global()
        .expect("Failed to build rayon thread pool");

    let count_cold_as_hit = false;

    // let data_path = "./out/clam/rit/medium/trace/3mm.csv";
    // blockit_and_opt_miss_ratio(data_path.into(), count_cold_as_hit);

    let traces =
        get_files_with_extension("../../loc_sys_mount/clam/plru_medium_l2b512/traces", "bin");

    // let mut traces =
    //     get_files_with_extension("../loc_sys_mount/clam/plru_medium_l2b512/traces", "bin");
    // let traces_gz =
    //     get_files_with_extension("../loc_sys_mount/clam/plru_medium_l2b512/traces", "gz");
    // traces.extend(traces_gz);

    // let traces = get_files_with_extension("../../loc_sys_mount/clam/plru_large/traces", "gz");

    if traces.is_empty() {
        log::warn!(
            "No trace files found in ../../loc_sys_mount/clam/plru_large/traces! Check path."
        );
    }

    let start = std::time::Instant::now(); // Start timing
    traces.par_iter().for_each(|trace| {
        if let Err(e) = blockit_and_opt_miss_ratio(trace.into(), count_cold_as_hit) {
            log::error!("Error processing {:?}: {}", trace, e);
        }
    });
    let elapsed = start.elapsed(); // End timing
    log::info!("Total execution time: {:.2?}", elapsed);
}
