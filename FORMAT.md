# BlockTrace format

The canonical output is `block_trace.bin.zst`. After zstd decompression it is
a headerless sequence of fixed-size, little-endian 12-byte records.

| Offset | Size | Type | Meaning |
| ---: | ---: | --- | --- |
| 0 | 4 | `u32` | Program counter or reference identifier |
| 4 | 4 | `i32` | Forward reuse interval |
| 8 | 4 | `u32` | Cache-block tag |

The forward reuse interval is the number of accesses from the current record
to the next record with the same block tag. `i32::MAX` means that the block is
not referenced again.

The address field is always a block tag. Input adapters must perform address
normalization before writing this format. For example, legacy word addresses
with 16 words per block are divided by 16, while ChampSim byte addresses are
divided by 64.

The current format preserves the historical layout so existing simulators can
consume files generated before the repository split. It has no magic bytes or
version header; consumers should reject decompressed lengths that are not a
multiple of 12.

## Legacy input

Legacy input is a sequence of 9-byte records:

| Offset | Size | Type | Meaning |
| ---: | ---: | --- | --- |
| 0 | 4 | `u32` | PC, little endian |
| 4 | 4 | `u32` | Address, little endian |
| 8 | 1 | `u8` | Native hit flag (`0` or nonzero) |

Raw `.bin` and gzip-compressed `.bin.gz` files are supported. Use
`--elements-per-block` to specify how input addresses map to cache blocks.

## ChampSim input

Raw `.champsimtrace` and xz-compressed `.champsimtrace.xz` files are
supported. Each instruction record is 64 bytes. Nonzero source memory
operands emit reads before nonzero destination memory operands emit writes;
register-only instructions are skipped. Byte addresses are converted to
64-byte cache-line tags.
