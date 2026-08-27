use clap::Parser;
use constructive_opt::block_it_for_bin::{self, ProcessedTrace};
use constructive_opt::champsim;
use constructive_opt::opt_miss_ratio;
use constructive_opt::utils::*;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::error::Error;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Compute OPT cache miss ratios from legacy binary or ChampSim traces"
)]
struct Cli {
    /// Directory containing the input traces.
    #[arg(default_value = "../../loc_sys_mount/pin_polybench/traces")]
    traces_dir: PathBuf,

    /// Write packed binary hit traces (one bit per access).
    ///
    /// This writes each OPT hit trace under `hit_traces/`. For legacy binary
    /// inputs, it also writes the native hit trace embedded in the input.
    #[arg(long)]
    write_hit_trace_bin: bool,

    /// Number of addressable elements in one cache block for legacy `.bin` traces.
    ///
    /// Addresses are divided by this value to obtain block tags. A value of 1
    /// means that an element is the same size as a cache block, or equivalently
    /// that each input address is already a block tag. ChampSim traces always
    /// use their native 64-byte cache-line size.
    #[arg(long, default_value_t = NonZeroU32::new(1).unwrap(), value_name = "N")]
    elements_per_block: NonZeroU32,
}

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
    write_hit_trace: bool,
    pb: &ProgressBar,
    bench_name: &str,
) -> Result<Vec<BenchResult>, Box<dyn Error>> {
    // let cache_sizes = [128, 512];
    // let cache_sizes = [32768, 49152, 65536, 131072, 204800, 262_144, 327_689]; // For GPU l2, 4mb, 6mb, 12mb,16mb, 20mb, 32mb, 40mb
    // i want to scan through the cache sizes from 16 to 10000 and skip the ones that's not divisible by 16
    let cache_sizes: Vec<usize> = (32..=4096).step_by(32).collect();

    let mut results = Vec::new();

    for &cache_size in &cache_sizes {
        pb.set_message(format!("{} — OPT_{cache_size}: running…", bench_name));

        let miss_result = opt_miss_ratio(
            &trace.block_tags,
            &trace.forward_refs,
            cache_size,
            count_cold_as_hit,
        );
        if write_hit_trace {
            let hit_trace_path = data_path.join("hit_traces");
            let trace_name = format!("OPT_{}", cache_size);
            write_hit_trace_bin(&hit_trace_path, &trace_name, &miss_result.hit_trace)?;
        }

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
    trace_format: TraceFormat,
    count_cold_as_hit: bool,
    elements_per_block: u32,
    write_hit_trace: bool,
    pb: &ProgressBar,
    // Counting semaphore: at most IO_CONCURRENCY threads read from the
    // remote drive simultaneously to avoid saturating network bandwidth.
    io_sem: &(Mutex<usize>, Condvar),
) -> Result<Vec<BenchResult>, Box<dyn Error>> {
    let file_name = data_path
        .file_name()
        .ok_or("Invalid file name")?
        .to_string_lossy();
    let bench_name = file_name
        .strip_suffix(".champsimtrace.xz")
        .or_else(|| file_name.strip_suffix(".champsimtrace"))
        .or_else(|| file_name.strip_suffix(".bin.gz"))
        .or_else(|| file_name.strip_suffix(".bin"))
        .unwrap_or(&file_name)
        .to_owned();

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
        let result = match trace_format {
            TraceFormat::LegacyBinary => block_it_for_bin::convert(
                &data_path,
                bench_result_dir.clone(),
                elements_per_block,
                write_hit_trace,
                pb,
            ),
            TraceFormat::ChampSim => champsim::convert(&data_path, &bench_result_dir, pb),
        };

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
        write_hit_trace,
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
    let cli = Cli::parse();

    // let parallelism = num_cpus::get_physical() / 2;
    let parallelism = 3;
    rayon::ThreadPoolBuilder::new()
        .num_threads(parallelism)
        .build_global()
        .expect("Failed to build rayon thread pool");

    let count_cold_as_hit = false;

    let traces_dir = cli.traces_dir;
    let (trace_format, traces) = match discover_traces(&traces_dir) {
        Ok(discovered) => discovered,
        Err(error) => {
            log::error!("Could not read {}: {}", traces_dir.display(), error);
            return;
        }
    };

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

        match blockit_and_opt_miss_ratio(
            trace.into(),
            trace_format,
            count_cold_as_hit,
            cli.elements_per_block.get(),
            cli.write_hit_trace_bin,
            &pb,
            &io_sem,
        ) {
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
            r.benchmark, r.cache_size, r.total_accesses, r.miss_count, r.miss_ratio
        );
    }
    println!("{:-<72}", "");

    // ── Write CSV ────────────────────────────────────────────────────────────
    if let Err(e) = write_csv(&results, &traces_dir) {
        log::error!("Failed to write CSV: {}", e);
    }

    log::info!("Total execution time: {:.2?}", elapsed);
}
