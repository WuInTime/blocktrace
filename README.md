# blocktrace

`blocktrace` normalizes memory traces into cache-block accesses and computes
the forward distance to the next access of each block. The generated trace can
be shared by cache simulators, lease simulators, and miss-ratio tools.

Supported input formats:

- Legacy 9-byte binary records, raw or gzip-compressed
- ChampSim 64-byte instruction records, raw or xz-compressed

Every conversion writes a canonical `block_trace.bin.zst`; addresses in that
file are already block tags. See [FORMAT.md](FORMAT.md) for the record layout.

## Command line

```bash
cargo run --release -- trace.bin.gz \
  --elements-per-block 16 \
  --output-directory results

cargo run --release -- trace.champsimtrace.xz \
  --output-directory results
```

The input format is inferred from the filename. Override it with
`--format legacy-bin` or `--format champsim` when necessary.

For legacy input, `--write-native-hit-trace` also writes the packed hit bits
embedded in the source trace.

## Rust library

```rust
use blocktrace::{ConvertOptions, TraceFormat, convert};

let trace = convert(
    "trace.bin.gz",
    "results",
    TraceFormat::LegacyBinary,
    ConvertOptions {
        elements_per_block: std::num::NonZeroU32::new(16).unwrap(),
        write_native_hit_trace: false,
    },
)?;

println!("{} cache accesses", trace.len());
# Ok::<(), Box<dyn std::error::Error>>(())
```

`BlockTrace::read_zstd` reads an existing normalized trace without decoding
the original input again.

## History

This project was extracted from `constructive_opt`, now named
[`belady-mrc`](https://github.com/WuInTime/belady-mrc). The original Git
history is intentionally retained.
