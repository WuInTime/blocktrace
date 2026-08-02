# constructive_opt

Constructive OPT cache analysis utilities for processing traces and computing OPT miss ratios.

## What This Project Does

- Reads input traces.
- Builds block-tag and forward-reference sequences.
- Computes OPT miss ratio for configured cache sizes.
- Writes per-size hit traces and a summary CSV.

## Current Default Address Semantics

The main executable currently treats input addresses as block addresses by default.

- In `src/main.rs`, execution goes through `block_it_for_bin::convert(...)`.
- In `src/block_it_for_bin.rs`, `BLOCK_SIZE` is set to `1`.
- With `BLOCK_SIZE == 1`, addresses are used directly as block tags (no shift/divide).

If you want word-address input instead, set `BLOCK_SIZE` to the number of words per block (for example `16`) in `src/block_it_for_bin.rs`.

## Run

Build and run with release optimizations. The optional argument is the trace
directory; it defaults to the PolyBench ChampSim trace directory.

```bash
cargo run -r
cargo run -r -- /path/to/traces
```

When every regular file in the directory ends in `.champsimtrace` or
`.champsimtrace.xz`, the input is decoded as 64-byte ChampSim instruction
records. Raw and xz-compressed traces are supported. Memory operands become
64-byte cache-line accesses; register-only instructions are skipped.

The program prints a miss-ratio summary table and writes output files under each benchmark result directory.

## Notes

- Trace format details are documented in `src/TRACE_FORMAT.md`.
- The currently selected trace root path is configured in `src/main.rs`.
