# davefs

Read-only userspace library and CLI for **Echostar / Dish DaveSystemDisk** (E\*FS) partitions. Open a raw partition file or a whole-disk image, parse the Dave headers, list inner substores, and hexdump bytes.

This is a forensic / research tool for disks the **owner already removed** from a **decommissioned DVR**. It does not write the image.

## Safety

- **Read-only I/O only.** Files are opened with write disabled. There is no format, repair, or inject command.
- **Not a Windows kernel driver.** Nothing here mounts E\*FS, installs a filesystem filter, or runs in kernel mode.
- **Not for a live set-top box.** Do not use this to write a disk the STB will boot or record onto. Do not flash firmware, dump a running box, or poke the boot chain.
- Work from a **copy** of the image when you can. Pointing the CLI at a USB-attached original is still read-only, but a copy is cheaper to get wrong.

If you need to extract recordings later, that work belongs in a later userspace decoder once the object index is known — not in a write path back onto the DVR disk.

## What this is (and is not)

ViP-class Dish / Echostar DVRs (forum notes for **622 / 722**, and the same family on later HD boxes) put Linux on some partitions and a proprietary object store on one large **0x83** partition. People nicknamed that store **Dave’s filesystem** after the on-disk strings `DaveSystemDisk` and `Long Live Dave!`. Forum lore: E\*FS holds **Misc / VOD / Recordings**, which maps to the inner names `EFSMisc` / `ES_RESERVED` / `AV_REQ_HD`.

`davefs` parses those headers offline. It does **not** yet walk allocation tables, list recordings, or demux MPEG. Those are research TODOs.

## Build and run

Rust 1.70+ (tested on 1.83). No extra system libraries.

```bash
cargo test
cargo build --release
./target/release/dave info /path/to/dave-partition.img
./target/release/dave substores /path/to/dave-partition.img
./target/release/dave hexdump /path/to/image.img --offset 0x10000 --length 256
```

Whole-disk dump (MBR + Linux partitions): the CLI looks for a **0x83** partition whose first bytes are the Dave superblock. Override if needed:

```bash
./target/release/dave info disk.img --partition-offset 64M
```

Offsets accept decimal, `0x` hex, and `K` / `KiB` / `M` / `MiB` / `G` / `GiB`.

| Command | Purpose |
| --- | --- |
| `dave info <path>` | Superblock: vendor, magic, sector count at +0x48, declared vs readable size |
| `dave substores <path>` | Inner stores found via ViP hints + a `Long Live Dave!` scan |
| `dave substores <path> --scan` | Scan the whole readable partition (slow on a 460 GiB dump) |
| `dave hexdump <path> --offset N` | Classic hex+ASCII from an **absolute file** offset (default 256 bytes) |

`hexdump --offset` is from the start of the file, matching a hex editor. `info` / `substores` offsets are from the **Dave partition** start.

## On-disk layout (observed)

Reference: one **ViP-class Hitachi** drive. Values below are from that image plus 622/722 forum notes. Anything marked **hypothesis** is unverified.

### Outer disk

Classic **MBR**. Partition type **0x83** (Linux). One large 0x83 partition is Dave; other 0x83 slots may be ext (data / `MISC_HD`) plus swap. This tool does not mount ext.

### Dave partition superblock (offset 0 of the Dave partition)

```
+0x00  ASCII  "Echostar Technologies Corp.\0"
+0x1C  ASCII  "DaveSystemDisk\0"
+0x2B  unknown (padding / fields not decoded)
+0x48  u32le  partition size in 512-byte sectors
             observed 966866850  ≈  460.85 GiB  (966866850 × 512)
```

The two C-strings and the +0x48 sector count are treated as **high confidence** on this family. Everything between `DaveSystemDisk\0` and +0x48, and everything after +0x4C, is still unknown.

### Inner substores

Each store begins with:

```
+0x00  ASCII  "Long Live Dave!\0"
+0x10  ASCII  name C-string  (AV_REQ_HD / ES_RESERVED / EFSMisc / …)
then   numeric fields, layout unknown
```

Published locations on the **reference ViP Hitachi** drive, measured from the Dave partition start:

| Name | Offset | Role (622/722 lore) | Notes |
| --- | --- | --- | --- |
| `AV_REQ_HD` | +64 MiB | Recordings | MPEG PES start code `00 00 01 E0` seen after the header on the reference drive |
| `ES_RESERVED` | +307264 MiB | VOD | |
| `EFSMisc` | +471104 MiB | Misc | May relate to ext `MISC_HD` / `catalog.cat` |

**Hypothesis:** a little-endian `u32` of `0x2000` (8192) appears in the `AV_REQ_HD` header and *may* be a block size. The parser reports it as `block_size_candidate` when present. Do not build an allocator on this yet.

Default `substores` behaviour:

1. Probe the three ViP offsets above, if they fall inside the file.
2. Scan the whole image if it is ≤ 16 MiB (covers the synthetic fixture).
3. On a large dump, scan only the first 128 MiB unless you pass `--scan`.

### Synthetic fixture

Tests embed the **real magic bytes** and the **reference sector count**, not a 460 GiB dump. Substores sit at compact offsets (64 / 96 / 128 KiB) so `cargo test` stays small. Generate the same image from Rust with `davefs::fixture::write_partition_fixture`.

## Library

```rust
use davefs::DaveImage;

let mut img = DaveImage::open("partition.img", None)?;
let sb = img.superblock();
let stores = img.substores(false)?;
```

`DaveImage::open` takes an optional partition byte offset. `None` means auto-detect (Dave at 0, or MBR 0x83).

## Next research steps

Leave these as follow-on work. The CLI already prints that the index is not decoded.

1. **Allocation / free maps** — dump the first few KiB after each `Long Live Dave!` header on a real image (`dave hexdump --offset …`) and look for repeating `u32`/`u64` runs, bitmaps, or 8192-byte structure if the block-size hypothesis holds.
2. **Object index** — find the table that maps recording / object IDs to block runs inside `AV_REQ_HD`. Until that exists, PES start codes are only a hint that video lives in that store.
3. **ext `MISC_HD` / `catalog.cat`** — mount the Linux partitions read-only (`mount -o ro,noload`) and correlate catalog entries with Dave object IDs. Do not write those ext volumes either.
4. **MPEG extract** — once extents are known, copy PES/TS out to a file. Expect CA / box-bound encryption on some content; this repo will not try to break that.
5. **Cross-box survey** — confirm +0x48 and the three MiB offsets on 622, 722, and later ViP / Hopper-class disks. Record differences instead of assuming the Hitachi layout is universal.

## TODOs in code

- Decode allocation tables.
- Decode the object index / listing.
- Link Dave objects to `catalog.cat`.
- Extract MPEG once extents are known.

## License

MIT. Use on images you are allowed to read.
