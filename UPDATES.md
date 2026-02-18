# Project Updates

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

## Planned Updates

- **Single-Pass Processing**: Merge Pass 1 (Hit Trace) and Pass 2 (Block Trace) to reduce IO and decompression overhead by 50%.
