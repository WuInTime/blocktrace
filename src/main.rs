use blocktrace::{ConvertOptions, TraceFormat, convert, detect_trace_format};
use clap::{Parser, ValueEnum};
use std::error::Error;
use std::num::NonZeroU32;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum FormatArg {
    Auto,
    LegacyBin,
    Champsim,
}

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Convert memory traces into canonical cache-block traces"
)]
struct Cli {
    /// Input .bin[.gz] or .champsimtrace[.xz] file.
    input: PathBuf,

    /// Directory in which block_trace.bin.zst will be written.
    #[arg(short, long, default_value = ".")]
    output_directory: PathBuf,

    /// Input format. By default it is inferred from the filename.
    #[arg(long, value_enum, default_value_t = FormatArg::Auto)]
    format: FormatArg,

    /// Number of addressable elements per block for legacy binary input.
    #[arg(long, default_value_t = NonZeroU32::MIN, value_name = "N")]
    elements_per_block: NonZeroU32,

    /// Write the hit trace embedded in legacy binary input.
    #[arg(long)]
    write_native_hit_trace: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let format = match cli.format {
        FormatArg::Auto => detect_trace_format(&cli.input)
            .ok_or_else(|| format!("cannot infer the input format from {}", cli.input.display()))?,
        FormatArg::LegacyBin => TraceFormat::LegacyBinary,
        FormatArg::Champsim => TraceFormat::ChampSim,
    };
    let trace = convert(
        &cli.input,
        &cli.output_directory,
        format,
        ConvertOptions {
            elements_per_block: cli.elements_per_block,
            write_native_hit_trace: cli.write_native_hit_trace,
        },
    )?;
    println!(
        "Wrote {} accesses to {}",
        trace.len(),
        cli.output_directory.join("block_trace.bin.zst").display()
    );
    Ok(())
}
