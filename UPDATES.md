# Project Updates

## 2026-08-02: ChampSim Trace Support

- **Change**: Added automatic trace-format detection. When every regular file in the input directory is a `.champsimtrace` or `.champsimtrace.xz` file, the directory is processed as ChampSim input; existing `.bin` and `.bin.gz` inputs continue to use the legacy reader.
- **Change**: Added a streaming reader for raw and xz-compressed 64-byte, little-endian ChampSim instruction records.
- **Change**: Skip register-only instructions and convert every nonzero memory operand into a simulator access. Source-memory operands emit reads before destination-memory operands emit writes, with slot order preserved within each group.
- **Change**: Convert ChampSim byte addresses to 64-byte cache-line addresses, calculate forward reuse intervals, and write `block_trace.bin.zst` output for the existing OPT pipeline.
- **Change**: Reject incomplete ChampSim records instead of silently dropping a partial final record.
- **Change**: Accept an optional trace-directory command-line argument and default to the PolyBench ChampSim trace directory.
- **Change**: Preserve the complete 32-bit PC in the legacy binary converter so its top-byte phase marker is not discarded during conversion.
- **Tests**: Added coverage for raw and xz-compressed input, register-only filtering, multiple operand slots, read-before-write ordering, and incomplete records.
- **Files**: `src/champsim.rs`, `src/utils.rs`, `src/main.rs`, `src/lib.rs`, `src/block_it_for_bin.rs`, `Cargo.toml`, `README.md`, `src/TRACE_FORMAT.md`, and `read.py`.

## 2026-03-16: Hit trace naming and debug cleanup

- **Change**: Derive `hit_trace_type` from the trace file's grandparent directory. Names like `plru_b512` or `clam_b128_medium_andrew` are now mapped to `plru_512` and `clam_128` respectively.
- **Change**: modified the block it for bin so that it only do one pass.
- **Files**: `src/block_it_for_bin.rs`

## 2026-02-18: RAM and IO Optimizations for Trace Processing

### 1. RAM Optimization: Split Vectors
- **Problem**: `Vec<(u32, u32)>` had 8 bytes per entry due to alignment padding.
- **Solution**: Split into `Vec<u16>` (PC) and `Vec<u32>` (Address) using Structure of Arrays (SoA).
- **Benefit**: Reduced memory usage by 2 bytes per entry (25% reduction for this data).

### 2. Write Optimization: Buffered Output
- **Problem**: 3 separate `write_all` calls (4 bytes each) per entry caused high function call overhead.
- **Solution**: Combined into a single 12-byte buffered write per entry.
- **Benefit**: Reduced CPU overhead during trace generation.

### 3. RAM Optimization: Forward Reuse Interval Calculation
- **Problem**: Storing all access indices in `HashMap<u32, Vec<usize>>` consumed O(N) memory (~8GB for 1B entries).
- **Solution**: Replaced with `HashMap<u32, usize>` to store only the *last* seen index. Forward RI is calculated on-the-fly (`curr_idx - prev_idx`) when the next access arrives.
- **Benefit**: Massive RAM reduction (O(M) memory where M is unique blocks).

### 4. RAM Optimization: Struct of Arrays (SoA) for Trace Structure
- **Problem**: `Vec<TraceEntry>` stored `u32` (block_tag) + `i32` (forward_ri) = 8 bytes per entry. This duplicated the data already in `processed_trace`.
- **Solution**: Refactored `TraceEntry` into separate vectors for `block_tags` and `forward_refs` using SoA. Removed the intermediate vector construction entirely.
- **Benefit**: Reduced memory usage by ~45% (8 bytes per entry) by avoiding redundant data structures. Now only raw vectors are kept in RAM.

## Planned Updates

- **Phase metadata for ChampSim traces**: Traditional `.bin.zst` traces reserve the most-significant byte of the 32-bit PC field for the phase ID, leaving the remaining three bytes for the PC. ChampSim records instead contain a complete 64-bit PC and no phase ID. A future fix must define how phase IDs are assigned and stored for ChampSim-derived traces without interpreting part of the real PC as phase metadata or truncating the PC.
