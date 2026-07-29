use constructive_opt::block_it_for_bin::{self, ProcessedTrace};
use constructive_opt::opt_miss_ratio;
use constructive_opt::utils::*;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

// One row of results per (benchmark × cache_size)
struct BenchResult {
    benchmark: String,
    cache_size: usize,
    total_accesses: usize,
    miss_count: usize,
    miss_ratio: f64,
}

fn generate_opt_miss_ratio_data(
    trace: &ProcessedTrace,
    data_path: &Path,
    count_cold_as_hit: bool,
    pb: &ProgressBar,
    bench_name: &str,
) -> Result<Vec<BenchResult>, Box<dyn Error>> {
    // let cache_sizes = [128, 512];
    // i want to scan through the cache sizes from 16 to 10000 and skip the ones that's not divisible by 16
    let cache_sizes: Vec<usize> = (16..=10000).step_by(16).collect();

    let mut results = Vec::new();

    for &cache_size in &cache_sizes {
        pb.set_message(format!("{} — OPT_{cache_size}: running…", bench_name));

        let miss_result = opt_miss_ratio(
            &trace.block_tags,
            &trace.forward_refs,
            cache_size,
            count_cold_as_hit,
        );

        // uncomment to store the hit trace for each cache size
        // let col_name = format!("OPT_{}", cache_size);
        // write_hit_trace_bin(data_path, &col_name, &miss_result.hit_trace)?;

        pb.set_message(format!(
            "{} — OPT_{}: {} misses ({:.2}%)",
            bench_name,
            cache_size,
            miss_result.miss_count,
            miss_result.miss_ratio * 100.0
        ));

        results.push(BenchResult {
            benchmark: bench_name.to_string(),
            cache_size,
            total_accesses: miss_result.cache_accesses,
            miss_count: miss_result.miss_count,
            miss_ratio: miss_result.miss_ratio * 100.0,
        });
    }

    Ok(results)
}

/// Read the remote trace (serialized: one thread at a time) then run OPT
/// (parallel: lock released before the CPU-bound computation).
fn blockit_and_opt_miss_ratio(
    data_path: PathBuf,
    count_cold_as_hit: bool,
    pb: &ProgressBar,
    // Counting semaphore: at most IO_CONCURRENCY threads read from the
    // remote drive simultaneously to avoid saturating network bandwidth.
    io_sem: &(Mutex<usize>, Condvar),
) -> Result<Vec<BenchResult>, Box<dyn Error>> {
    let bench_name = data_path
        .file_stem()
        .ok_or("Invalid file stem")?
        .to_string_lossy()
        .into_owned();

    let out_directory = data_path
        .parent()
        .ok_or("Invalid data path parent")?
        .parent()
        .ok_or("Invalid data path grand-parent")?
        .join("results");

    let bench_result_dir = out_directory.join(&bench_name);

    // ── Phase 1: I/O  (≤ IO_CONCURRENCY threads at a time) ─────────────────
    pb.set_message(format!("{} — waiting for I/O slot…", bench_name));
    let trace: ProcessedTrace = {
        let (lock, cvar) = io_sem;
        // Acquire: wait until a slot is free, then decrement the counter.
        let mut slots = cvar.wait_while(lock.lock().unwrap(), |s| *s == 0).unwrap();
        *slots -= 1;
        drop(slots); // release the mutex while doing I/O

        pb.set_message(format!("{} — reading remote trace…", bench_name));
        let result = block_it_for_bin::convert(&data_path, bench_result_dir.clone(), pb);

        // Release: increment the counter and wake one waiter.
        *lock.lock().unwrap() += 1;
        cvar.notify_one();

        result?
    };

    // ── Phase 2: CPU-bound OPT computation (fully parallel) ─────────────────
    let results = generate_opt_miss_ratio_data(
        &trace,
        &bench_result_dir,
        count_cold_as_hit,
        pb,
        &bench_name,
    )?;

    pb.finish_with_message(format!("{} — done ✓", bench_name));
    Ok(results)
}

fn write_csv(results: &[BenchResult], traces_dir: &Path) -> Result<(), Box<dyn Error>> {
    // Place the CSV as a sibling of the traces/ directory
    let out_path = traces_dir
        .parent()
        .ok_or("traces dir has no parent")?
        .join("opt_summary.csv");

    let mut wtr = csv::Writer::from_path(&out_path)?;
    wtr.write_record([
        "benchmark",
        "cache_size",
        "total_accesses",
        "miss_count",
        "miss_ratio",
    ])?;

    for r in results {
        wtr.write_record(&[
            r.benchmark.clone(),
            r.cache_size.to_string(),
            r.total_accesses.to_string(),
            r.miss_count.to_string(),
            format!("{:.6}", r.miss_ratio),
        ])?;
    }
    wtr.flush()?;

    println!("\nSummary written to: {}", out_path.display());
    Ok(())
}

pub fn main() {
    env_logger::init();

    // let parallelism = num_cpus::get_physical() / 2;
    let parallelism = 3;
    rayon::ThreadPoolBuilder::new()
        .num_threads(parallelism)
        .build_global()
        .expect("Failed to build rayon thread pool");

    let count_cold_as_hit = false;

    // let traces_dir = Path::new("../../loc_sys_mount/clam/test/traces");
    // let traces_dir = Path::new("../../loc_sys_mount/l2_block/plru_b512_regl_small/traces");
    let traces_dir = Path::new("../../loc_sys_mount/shuang_zhai/gemv2t/traces");
    let traces = get_files_with_extension(traces_dir, "bin");

    if traces.is_empty() {
        log::warn!(
            "No trace files found in {}! Check path.",
            traces_dir.display()
        );
        return;
    }

    // ── Multi-progress: one spinner row per benchmark ────────────────────────
    let mp = MultiProgress::new();
    let spinner_style = ProgressStyle::with_template("{spinner:.cyan} [{elapsed_precise}] {msg}")
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✓"]);

    let all_results: Mutex<Vec<BenchResult>> = Mutex::new(Vec::new());

    // Counting semaphore: at most IO_CONCURRENCY threads read from the
    // remote drive simultaneously.
    const IO_CONCURRENCY: usize = 2;
    let io_sem: Arc<(Mutex<usize>, Condvar)> =
        Arc::new((Mutex::new(IO_CONCURRENCY), Condvar::new()));

    let start = std::time::Instant::now();

    traces.par_iter().for_each(|trace| {
        let pb = mp.add(ProgressBar::new_spinner());
        pb.set_style(spinner_style.clone());
        pb.enable_steady_tick(std::time::Duration::from_millis(80));

        match blockit_and_opt_miss_ratio(trace.into(), count_cold_as_hit, &pb, &io_sem) {
            Ok(results) => {
                all_results.lock().unwrap().extend(results);
            }
            Err(e) => {
                pb.finish_with_message(format!(
                    "✗ {:?}: {}",
                    trace.file_name().unwrap_or_default(),
                    e
                ));
                log::error!("Error processing {:?}: {}", trace, e);
            }
        }
    });

    let elapsed = start.elapsed();

    // Collect and sort results for a tidy summary
    let mut results = all_results.into_inner().unwrap();
    results.sort_by(|a, b| {
        a.benchmark
            .cmp(&b.benchmark)
            .then(a.cache_size.cmp(&b.cache_size))
    });

    // ── Print summary table ──────────────────────────────────────────────────
    println!("\n{:-<72}", "");
    println!(
        "{:<20} {:>10} {:>16} {:>10} {:>8}",
        "Benchmark", "Cache", "Accesses", "Misses", "Miss%"
    );
    println!("{:-<72}", "");
    for r in &results {
        println!(
            "{:<20} {:>10} {:>16} {:>10} {:>7.2}%",
            r.benchmark,
            r.cache_size,
            r.total_accesses,
            r.miss_count,
            r.miss_ratio
        );
    }
    println!("{:-<72}", "");

    // ── Write CSV ────────────────────────────────────────────────────────────
    if let Err(e) = write_csv(&results, traces_dir) {
        log::error!("Failed to write CSV: {}", e);
    }

    log::info!("Total execution time: {:.2?}", elapsed);
}
