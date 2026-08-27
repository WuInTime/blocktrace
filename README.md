# constructive_opt

Constructive OPT cache analysis utilities for processing traces and computing OPT miss ratios.

## What This Project Does

- Reads input traces.
- Builds block-tag and forward-reference sequences.
- Computes OPT miss ratio for configured cache sizes.
- Writes a summary CSV and, when requested, per-size hit traces.

## Address Semantics

Legacy `.bin` input addresses are interpreted using `--elements-per-block`,
which defaults to `1`.

- `--elements-per-block 1` means one element has the same size as one cache
  block. Equivalently, the input address is already a block tag, so it is used
  directly.
- For word-addressed input with 16 words per cache block, use
  `--elements-per-block 16`. Each input address is then divided by 16 to obtain
  its block tag.

ChampSim inputs retain their native 64-byte cache-line interpretation; this
option only affects legacy `.bin` traces.

## Run

Build and run with release optimizations. The optional argument is the trace
directory; it defaults to the PolyBench ChampSim trace directory.

```bash
cargo run -r
cargo run -r -- /path/to/traces
cargo run -r -- /path/to/traces --elements-per-block 16
cargo run -r -- /path/to/traces --elements-per-block 16 --write-hit-trace-bin
cargo run -r -- /path/to/traces --write-hit-trace-bin
```

Hit traces are disabled by default. `--write-hit-trace-bin` writes packed OPT
hit traces (one bit per access) under each benchmark's `hit_traces/` directory.
For legacy input, it also stores the native hit trace embedded in the source
trace in the benchmark result directory.

When every regular file in the directory ends in `.champsimtrace` or
`.champsimtrace.xz`, the input is decoded as 64-byte ChampSim instruction
records. Raw and xz-compressed traces are supported. Memory operands become
64-byte cache-line accesses; register-only instructions are skipped.

The program prints a miss-ratio summary table and writes output files under each benchmark result directory.

## Notes

- Trace format details are documented in `src/TRACE_FORMAT.md`.
- The currently selected trace root path is configured in `src/main.rs`.
