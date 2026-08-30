"""Read fixed-size ChampSim instruction-trace records.

Each record represents one dynamically executed instruction.  The format used
here is ``<QBB2B4B2Q4Q``: ``<`` means little-endian, ``B`` is a one-byte
unsigned integer, and ``Q`` is an eight-byte unsigned integer.

Byte layout (64 bytes total):

    Offset  Size  Field
       0      8   Instruction pointer/program counter (PC)
       8      1   Is this instruction a branch? (0 or 1)
       9      1   Was the branch taken? (0 or 1)
      10      2   Up to 2 destination-register IDs (2 x 1 byte)
      12      4   Up to 4 source-register IDs      (4 x 1 byte)
      16     16   Up to 2 destination-memory addresses (2 x 8 bytes)
      32     32   Up to 4 source-memory addresses      (4 x 8 bytes)

The size is therefore 8 + 1 + 1 + 2 + 4 + 16 + 32 = 64 bytes.

Why "up to" several operands instead of exactly one?
Different instructions use different numbers of inputs and outputs.  For
example, ``add rax, rbx`` reads both rax and rbx, then writes rax, so rax is
both a source and a destination register.  Loads read memory and stores write
memory, while more complex instructions can access several registers or
memory locations.  ChampSim reserves a fixed maximum number of slots so every
record has the same size and can be decoded quickly.  Unused slots contain
zero; ``nonzero`` removes those empty slots when displaying a record.

Register fields contain register identifiers, not register contents.  Memory
fields contain effective addresses, not the values loaded or stored.  The PC
is the address of the instruction itself, not a data address.  ``taken`` is
meaningful for a branch; for a non-branch it is normally zero.

The record does not include instruction opcodes or assembly text.
"""

import argparse
import lzma
import struct
from pathlib import Path

record = struct.Struct("<QBB2B4B2Q4Q")

parser = argparse.ArgumentParser(description="Read a ChampSim trace file.")
parser.add_argument(
    "trace",
    type=Path,
    nargs="?",
    default=Path("gemm.champsimtrace.xz"),
    help="path to a .champsimtrace or .champsimtrace.xz file",
)
parser.add_argument(
    "--include-register-only",
    action="store_true",
    help=(
        "include instructions whose nonzero operands are only registers "
        "(skipped by default)"
    ),
)
parser.add_argument(
    "--max-records",
    type=int,
    default=100,
    help="maximum number of matching records to print (default: 100)",
)
args = parser.parse_args()

if args.max_records < 0:
    parser.error("--max-records must be nonnegative")

def nonzero(values, hexadecimal=False):
    if hexadecimal:
        return [f"{x:#x}" for x in values if x]
    return [x for x in values if x]

open_trace = lzma.open if args.trace.suffix.lower() == ".xz" else open

memory_access_records = 0
printed_records = 0
with open_trace(args.trace, "rb") as stream:
    index = 0
    while True:
        data = stream.read(record.size)
        if not data:
            break
        if len(data) != record.size:
            raise ValueError(
                f"incomplete record at index {index}: "
                f"expected {record.size} bytes, got {len(data)}"
            )

        fields = record.unpack(data)
        record_index = index
        index += 1

        # An instruction is register-only when it has no nonzero source or
        # destination memory address. These instructions do not represent a
        # cache access, so omit them unless the caller explicitly asks for
        # the complete instruction stream.
        has_memory_access = any(fields[9:15])
        if has_memory_access:
            memory_access_records += 1
        if not args.include_register_only and not has_memory_access:
            continue
        if printed_records >= args.max_records:
            continue

        print(
            f"{record_index:4}: "
            f"PC={fields[0]:#x} "
            f"branch={fields[1]} "
            f"taken={fields[2]} "
            f"dst_reg={nonzero(fields[3:5])} "
            f"src_reg={nonzero(fields[5:9])} "
            f"dst_mem={nonzero(fields[9:11], True)} "
            f"src_mem={nonzero(fields[11:15], True)}"
        )
        printed_records += 1

print(f"Total non-register-only records: {memory_access_records}")

        
# So the -t 10000000 cap worked exactly.

# Because you compiled with -fno-pie -no-pie, you can map those stable PCs back to GEMM assembly:
# objdump -d ../bin-medium/gemm \
#   --start-address=0x400740 \
#   --stop-address=0x400780


# Or map a PC to source code:
# addr2line -e ../bin-medium/gemm -f -C 0x400740
