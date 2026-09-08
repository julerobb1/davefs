//! Observed on-disk constants for DaveSystemDisk / E*FS.
//!
//! Offsets and interpretations marked **hypothesis** were seen on one ViP-class
//! Hitachi drive and have not been confirmed across receivers.

/// Classic 512-byte disk sector. Used for MBR LBA math and the superblock
/// sector-count field at +0x48.
pub const SECTOR_SIZE: u64 = 512;

/// ASCII vendor C-string at byte 0 of a Dave partition.
pub const VENDOR_CSTR: &[u8] = b"Echostar Technologies Corp.\0";

/// ASCII volume magic immediately after [`VENDOR_CSTR`].
pub const DISK_MAGIC_CSTR: &[u8] = b"DaveSystemDisk\0";

/// ASCII marker at the start of each inner substore.
pub const SUBSTORE_MAGIC_CSTR: &[u8] = b"Long Live Dave!\0";

/// Superblock field: little-endian u32 partition size in 512-byte sectors.
/// Observed value on the reference ViP Hitachi image: 966_866_850
/// (≈ 460.85 GiB). Treat the *meaning* as high-confidence; the rest of the
/// superblock after the two C-strings is still unknown.
pub const SUPERBLOCK_SECTOR_COUNT_OFFSET: usize = 0x48;

/// Bytes we always slurp for the partition superblock.
pub const SUPERBLOCK_READ_LEN: usize = 128;

/// Bytes we slurp for a substore header.
pub const SUBSTORE_HEADER_LEN: usize = 128;

/// MPEG-2 PES video stream id observed after the `AV_REQ_HD` header on the
/// reference drive. Not a guarantee that recordings are unencrypted PES.
pub const MPEG_PES_VIDEO_START: &[u8] = &[0x00, 0x00, 0x01, 0xE0];

/// Hypothesis: a little-endian u32 equal to this value in a substore header
/// may be a block size (8192). Documented, not trusted.
pub const BLOCK_SIZE_CANDIDATE: u32 = 0x2000;

/// 1 MiB in bytes (binary).
pub const MIB: u64 = 1024 * 1024;

/// Known inner-substore locations on the reference ViP-class Hitachi drive,
/// measured from the start of the Dave partition (not the whole disk).
///
/// Forum lore for 622/722 maps these names onto E*FS Misc / VOD / Recordings.
pub const VIP_SUBSTORE_HINTS: &[SubstoreHint] = &[
    SubstoreHint {
        name: "AV_REQ_HD",
        offset_mib: 64,
        role: crate::substore::SubstoreRole::Recordings,
        note: "recording store; MPEG PES 0xE0 observed after header on the reference drive",
    },
    SubstoreHint {
        name: "ES_RESERVED",
        offset_mib: 307_264,
        role: crate::substore::SubstoreRole::Vod,
        note: "forum lore: VOD store on 622/722",
    },
    SubstoreHint {
        name: "EFSMisc",
        offset_mib: 471_104,
        role: crate::substore::SubstoreRole::Misc,
        note: "forum lore: Misc store; may relate to ext MISC_HD catalog.cat",
    },
];

/// A published (name, MiB offset) pair from the reference drive.
#[derive(Debug, Clone, Copy)]
pub struct SubstoreHint {
    pub name: &'static str,
    pub offset_mib: u64,
    pub role: crate::substore::SubstoreRole,
    pub note: &'static str,
}

impl SubstoreHint {
    pub fn offset_bytes(&self) -> u64 {
        self.offset_mib.saturating_mul(MIB)
    }
}
