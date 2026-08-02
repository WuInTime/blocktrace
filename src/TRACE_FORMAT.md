# Trace Input Format for `lru_sim`

This simulator expects a root directory containing one subdirectory per benchmark or workload.

```text
root/
  benchmark_a/
    word_trace.bin.zst
  benchmark_b/
    block_trace.bin.zst
```

For each benchmark subdirectory, provide exactly one of these files:

- `word_trace.bin.zst`
- `block_trace.bin.zst`

If both exist, `word_trace.bin.zst` is used. If neither exists, the simulator stops with an error.

## Compression

Trace files must be compressed with `zstd`.

The simulator opens the file as zstd-compressed data and reads the decompressed byte stream.

## ChampSim Input

The main executable also accepts a directory containing only raw
`.champsimtrace` and/or xz-compressed `.champsimtrace.xz` files. Each
little-endian instruction record is 64 bytes: an 8-byte PC, two branch bytes,
two destination-register bytes, four source-register bytes, two destination
memory addresses, and four source memory addresses.

Zero memory operands and instructions with no memory operands are skipped.
Nonzero source operands emit reads before nonzero destination operands emit
writes, with slot order preserved. Byte addresses are converted to 64-byte
cache-line addresses.

## Binary Record Layout

The decompressed trace is a sequence of fixed-size records.

reuse interval is the gap between two consecutive accesses to the same item, while reuse distance is the number of distinct other items touched between those accesses.

Each record is exactly 12 bytes:

```text
byte offset  size  meaning
0            4     pc, little-endian unsigned 32-bit integer
4            4     forward reuse interval (next-use distance), little-endian signed 32-bit integer
8            4     address, little-endian unsigned 32-bit integer
```

The PC is stored in bytes 0 through 3, the forward reuse interval in bytes 4 through 7, and the address in bytes 8 through 11.

The address must be encoded as a little-endian `u32`.

Example record:

```text
78 56 34 12 ff ff ff 7f  34 12 00 00
```

This record has PC `0x12345678`, no future re-reference recorded yet (`i32::MAX`, bytes `ff ff ff 7f`), and address `0x00001234`.

## Address Meaning

The meaning of the address depends on the file name.

### `word_trace.bin.zst`

The address field is a word address.

The simulator converts it to a cache block address with:

```text
block_address = word_address >> 4
```

This means each cache block contains 16 words.

If the source trace has byte addresses and one word is 4 bytes:

```text
word_address = byte_address >> 2
```

### `block_trace.bin.zst`

The address field is already a block address.

The simulator uses it directly:

```text
block_address = address
```

If the source trace has byte addresses, one word is 4 bytes, and each block contains 16 words:

```text
block_address = byte_address >> 6
```

That is equivalent to dividing by 64 bytes per block.

## Running the Simulator

Example:

```bash
cargo run -- --root path/to/root --sizes 128,512
```

For each benchmark directory and cache size, the simulator writes:

```text
hit_trace_lru_<size>.bin
```

The output hit trace is bit-packed, MSB-first:

- `1` means cache hit
- `0` means cache miss
