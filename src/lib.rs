//! Read-only userspace library for Echostar/Dish **DaveSystemDisk** (E*FS)
//! partition images.
//!
//! This is a forensic / research parser for disks the owner already removed
//! from a decommissioned DVR. It never writes to the image.
//!
//! It is **not** a Windows kernel driver, **not** a live set-top-box tool, and
//! it does not implement allocation-table walking or MPEG extraction yet.

mod constants;
mod error;
pub mod fixture;
mod image;
mod mbr;
mod substore;
mod superblock;
mod util;

pub use constants::{
    SubstoreHint, BLOCK_SIZE_CANDIDATE, DISK_MAGIC_CSTR, MIB, MPEG_PES_VIDEO_START, SECTOR_SIZE,
    SUBSTORE_HEADER_LEN, SUBSTORE_MAGIC_CSTR, SUPERBLOCK_READ_LEN, SUPERBLOCK_SECTOR_COUNT_OFFSET,
    VENDOR_CSTR, VIP_SUBSTORE_HINTS,
};
pub use error::{Error, Result};
pub use image::{DaveImage, ImageKind};
pub use mbr::{Mbr, MbrPartition};
pub use substore::{Substore, SubstoreRole};
pub use superblock::Superblock;
pub use util::{hexdump, parse_bytes};

// TODO: decode the Dave allocation / free-block tables once the on-disk
//       structure past the `Long Live Dave!` header is known.
// TODO: decode the object index that maps recording IDs to block runs.
// TODO: cross-link objects to the ext `MISC_HD` `catalog.cat` (and related
//       metadata on the Linux partitions of a ViP disk).
// TODO: extract MPEG PES / TS from `AV_REQ_HD` once extents are known.
//       Do not assume recordings are unencrypted.

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn superblock_parses_reference_header_bytes() {
        let mut buf = vec![0u8; 128];
        buf[..VENDOR_CSTR.len()].copy_from_slice(VENDOR_CSTR);
        buf[VENDOR_CSTR.len()..VENDOR_CSTR.len() + DISK_MAGIC_CSTR.len()]
            .copy_from_slice(DISK_MAGIC_CSTR);
        buf[SUPERBLOCK_SECTOR_COUNT_OFFSET..SUPERBLOCK_SECTOR_COUNT_OFFSET + 4]
            .copy_from_slice(&fixture::REFERENCE_SECTOR_COUNT.to_le_bytes());

        let sb = Superblock::parse(&buf).unwrap();
        assert_eq!(sb.vendor, "Echostar Technologies Corp.");
        assert_eq!(sb.magic, "DaveSystemDisk");
        assert_eq!(sb.sector_count, 966_866_850);
        assert_eq!(sb.declared_size_bytes(), 966_866_850 * 512);
    }

    #[test]
    fn superblock_rejects_random_bytes() {
        assert!(Superblock::parse(&[0u8; 128]).is_err());
    }

    #[test]
    fn substore_parses_long_live_dave_and_name() {
        let mut hdr = vec![0u8; 128];
        hdr[..SUBSTORE_MAGIC_CSTR.len()].copy_from_slice(SUBSTORE_MAGIC_CSTR);
        let name = b"AV_REQ_HD\0";
        hdr[SUBSTORE_MAGIC_CSTR.len()..SUBSTORE_MAGIC_CSTR.len() + name.len()]
            .copy_from_slice(name);
        hdr[0x20..0x24].copy_from_slice(&BLOCK_SIZE_CANDIDATE.to_le_bytes());

        let ss = Substore::parse(64 * MIB, &hdr, MPEG_PES_VIDEO_START).unwrap();
        assert_eq!(ss.name, "AV_REQ_HD");
        assert_eq!(ss.role, SubstoreRole::Recordings);
        assert_eq!(ss.block_size_candidate, Some(0x2000));
        assert_eq!(ss.block_size_candidate_at, Some(0x20));
        assert!(ss.mpeg_pes_e0_after_header);
    }

    #[test]
    fn fixture_round_trip_in_memory() {
        let mut cur = Cursor::new(Vec::new());
        fixture::write_partition_fixture(&mut cur).unwrap();
        let bytes = cur.into_inner();
        assert!(bytes.starts_with(VENDOR_CSTR));
        assert_eq!(
            &bytes[VENDOR_CSTR.len()..VENDOR_CSTR.len() + DISK_MAGIC_CSTR.len()],
            DISK_MAGIC_CSTR
        );
        let count = u32::from_le_bytes(
            bytes[SUPERBLOCK_SECTOR_COUNT_OFFSET..SUPERBLOCK_SECTOR_COUNT_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(count, fixture::REFERENCE_SECTOR_COUNT);

        let av = fixture::FIXTURE_AV_REQ_HD_OFFSET as usize;
        assert!(bytes[av..].starts_with(SUBSTORE_MAGIC_CSTR));
        assert!(bytes[av + 128..].starts_with(MPEG_PES_VIDEO_START));
    }
}
