# OPC and ZIP Substrate

This document describes the implemented bounded ZIP layer in `wasmppt-opc`. Higher-level
Open Packaging Conventions (OPC) relationships and content types are introduced by the
next architecture slice.

## Implemented behavior

`ZipArchive` locates the End of Central Directory (EOCD) record and indexes the central
directory through the host-neutral `ReadAt` capability. Opening a package reads metadata
but does not inflate any entry. `read_entry` inflates one selected Stored or Deflate entry
and validates its declared size and CRC-32.

`ZipWriter` writes to the forward-only `OutputSink` capability. It never seeks. A raw-copy
operation rebuilds the local header and later central-directory record while copying the
original compressed payload in 64 KiB chunks. It clears data-descriptor state because all
sizes are known from the validated source index. A no-op rewrite therefore inflates and
recompresses zero entries, including entries with compression methods the reader cannot
decode.

Changed entries are encoded as Stored or raw Deflate data. They are compressed into a
temporary buffer before their header is emitted, which keeps the public output capability
strictly forward-only.

`StreamingZipWriter` implements the same deterministic format as `ZipWriter` through bounded
pulls. It streams raw entry payloads straight from `ReadAt`, retains only the active changed
entry's compressed bytes, and emits the central directory after all entries. A byte-for-byte
parity test drains it in seven-byte chunks; template generation separately tests one-byte pulls.

## Deterministic mode

Deterministic rewrites:

- order entries by UTF-8 name bytes;
- use the DOS timestamp `1980-01-01 00:00:00`;
- remove archive, entry, and extra-field comments and metadata;
- use fixed ZIP version and attribute fields; and
- preserve unchanged compressed payload bytes verbatim.

Repeated native runs over the same source are byte-identical. Future compression changes
MUST add cross-runtime golden tests before changing this contract.

## Security and limits

The reader rejects encrypted, multi-disk, and ZIP64 packages. It rejects duplicate or
unsafe paths, inconsistent local and central metadata, invalid data descriptors,
overlapping local records, truncated ranges, and entries overlapping the central
directory. Configurable `PackageLimits` apply to entry count, central-directory bytes,
compressed and uncompressed totals, individual inflated size, compression ratio, names,
extra fields, and comments before payload inflation.

CRC checking intentionally occurs when an entry is read; raw-copy does not spend CPU
inflating unchanged payloads. Higher layers MUST read and validate every semantic part
they interpret or mutate.

## Memory budget

With an external `ReadAt` source, indexing uses an EOCD tail buffer of at most 65,557
bytes, the bounded central directory (32 MiB by default), short local-header buffers, and
`O(entry count + metadata)` retained index memory. The source itself need not be copied.
`MemorySource`, used for browser `ArrayBuffer` input, retains the caller-provided package
buffer once.

Push-based raw-copy rewriting adds a fixed 64 KiB copy buffer plus `O(entry count + metadata)`
central records. The pull writer replaces that fixed copy buffer with the host-requested output
chunk. Writing a changed entry temporarily retains its input and compressed output;
reading an entry temporarily retains compressed plus uncompressed bytes. Default limits
cap a package at 10,000 entries, 512 MiB compressed total, 2 GiB uncompressed total, and
256 MiB per inflated entry. Hosts with tighter memory ceilings SHOULD supply lower limits.

Classic ZIP's 32-bit offsets and sizes are the current format ceiling. ZIP64 support is a
future compatibility decision, not an implicit unbounded allocation path.

## Verification

Unit and property tests cover Stored and Deflate round trips, verbatim compressed copying,
non-seekable output, deterministic output, limits, unsafe paths, CRC failures, and arbitrary
malformed byte strings. The libFuzzer target is at
`crates/wasmppt-opc/fuzz/fuzz_targets/open_package.rs` and can be run with:

```sh
cargo install cargo-fuzz
cargo fuzz run --fuzz-dir crates/wasmppt-opc/fuzz open_package
```

## Related documents

- See the [system architecture](architecture.md) for package-layer ownership and mutation
  policy.
- See the [development guide](develop.md) for the complete repository verification suite.
- Return to the [documentation index](index.md) for the project map.
