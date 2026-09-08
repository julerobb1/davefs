//! Synthetic DaveSystemDisk image used by tests and as a researcher template.
//!
//! This is **not** a dump from a DVR. It embeds the observed magic strings and
//! the reference sector-count field so parsers can be exercised offline.

use std::io::{self, Seek, SeekFrom, Write};

use crate::constants::{
    BLOCK_SIZE_CANDIDATE, DISK_MAGIC_CSTR, MPEG_PES_VIDEO_START, SUBSTORE_MAGIC_CSTR,
    SUPERBLOCK_SECTOR_COUNT_OFFSET, VENDOR_CSTR,
};

/// Sector count observed on the reference ViP-class Hitachi Dave partition.
pub const REFERENCE_SECTOR_COUNT: u32 = 966_866_850;

/// Compact fixture layout (partition image, no MBR).
pub const FIXTURE_AV_REQ_HD_OFFSET: u64 = 0x1_0000; // 64 KiB
pub const FIXTURE_ES_RESERVED_OFFSET: u64 = 0x1_8000; // 96 KiB
pub const FIXTURE_EFS_MISC_OFFSET: u64 = 0x2_0000; // 128 KiB
pub const FIXTURE_LEN: u64 = 0x3_0000; // 192 KiB

/// Hypothesis placement: LE u32 `0x2000` at this offset inside a substore header.
pub const FIXTURE_BLOCK_SIZE_FIELD_OFFSET: usize = 0x20;

/// Write a tiny partition image that starts with a real-looking Dave superblock
/// and three `Long Live Dave!` substores at compact offsets.
pub fn write_partition_fixture<W: Write + Seek>(mut w: W) -> io::Result<u64> {
    w.seek(SeekFrom::Start(0))?;
    w.write_all(&vec![0u8; FIXTURE_LEN as usize])?;

    write_superblock(&mut w, 0, REFERENCE_SECTOR_COUNT)?;
    write_substore(&mut w, FIXTURE_AV_REQ_HD_OFFSET, b"AV_REQ_HD", true)?;
    write_substore(&mut w, FIXTURE_ES_RESERVED_OFFSET, b"ES_RESERVED", false)?;
    write_substore(&mut w, FIXTURE_EFS_MISC_OFFSET, b"EFSMisc", false)?;
    w.flush()?;
    Ok(FIXTURE_LEN)
}

/// Whole-disk fixture: MBR + one Linux 0x83 partition starting at LBA 8.
pub const MBR_FIXTURE_PARTITION_LBA: u32 = 8;
pub const MBR_FIXTURE_PARTITION_OFFSET: u64 =
    MBR_FIXTURE_PARTITION_LBA as u64 * crate::constants::SECTOR_SIZE;

pub fn write_whole_disk_fixture<W: Write + Seek>(mut w: W) -> io::Result<u64> {
    let part_off = MBR_FIXTURE_PARTITION_OFFSET;
    let total = part_off + FIXTURE_LEN;
    w.seek(SeekFrom::Start(0))?;
    w.write_all(&vec![0u8; total as usize])?;

    write_mbr(
        &mut w,
        MBR_FIXTURE_PARTITION_LBA,
        (FIXTURE_LEN / 512) as u32,
    )?;
    write_superblock(&mut w, part_off, REFERENCE_SECTOR_COUNT)?;
    write_substore(
        &mut w,
        part_off + FIXTURE_AV_REQ_HD_OFFSET,
        b"AV_REQ_HD",
        true,
    )?;
    w.flush()?;
    Ok(total)
}

fn write_superblock<W: Write + Seek>(w: &mut W, at: u64, sector_count: u32) -> io::Result<()> {
    w.seek(SeekFrom::Start(at))?;
    w.write_all(VENDOR_CSTR)?;
    w.write_all(DISK_MAGIC_CSTR)?;
    w.seek(SeekFrom::Start(at + SUPERBLOCK_SECTOR_COUNT_OFFSET as u64))?;
    w.write_all(&sector_count.to_le_bytes())?;
    Ok(())
}

fn write_substore<W: Write + Seek>(
    w: &mut W,
    at: u64,
    name: &[u8],
    with_pes: bool,
) -> io::Result<()> {
    w.seek(SeekFrom::Start(at))?;
    w.write_all(SUBSTORE_MAGIC_CSTR)?;
    w.write_all(name)?;
    w.write_all(&[0])?;
    w.seek(SeekFrom::Start(at + FIXTURE_BLOCK_SIZE_FIELD_OFFSET as u64))?;
    w.write_all(&BLOCK_SIZE_CANDIDATE.to_le_bytes())?;
    if with_pes {
        // Dummy PES video packet immediately after the 128-byte header region.
        w.seek(SeekFrom::Start(at + 128))?;
        w.write_all(MPEG_PES_VIDEO_START)?;
        w.write_all(&[0x00, 0x10])?; // fake PES length
        w.write_all(b"synthetic-pes-payload")?;
    }
    Ok(())
}

fn write_mbr<W: Write + Seek>(w: &mut W, start_lba: u32, sector_count: u32) -> io::Result<()> {
    let mut sector = [0u8; 512];
    // One Linux 0x83 partition in slot 0.
    let entry = 0x1BE;
    sector[entry] = 0x00; // not bootable
    sector[entry + 4] = 0x83;
    sector[entry + 8..entry + 12].copy_from_slice(&start_lba.to_le_bytes());
    sector[entry + 12..entry + 16].copy_from_slice(&sector_count.to_le_bytes());
    sector[0x1FE] = 0x55;
    sector[0x1FF] = 0xAA;
    w.seek(SeekFrom::Start(0))?;
    w.write_all(&sector)?;
    Ok(())
}
