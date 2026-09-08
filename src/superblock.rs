use crate::constants::{
    DISK_MAGIC_CSTR, SECTOR_SIZE, SUPERBLOCK_READ_LEN, SUPERBLOCK_SECTOR_COUNT_OFFSET, VENDOR_CSTR,
};
use crate::error::{Error, Result};
use crate::util::read_cstr;

/// Parsed Dave partition superblock (offset 0 of the Dave partition).
#[derive(Debug, Clone)]
pub struct Superblock {
    pub vendor: String,
    pub magic: String,
    /// Little-endian u32 at +0x48. Observed to match the partition size in
    /// 512-byte sectors on the reference ViP Hitachi drive.
    pub sector_count: u32,
    pub raw: Vec<u8>,
}

impl Superblock {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < SUPERBLOCK_SECTOR_COUNT_OFFSET + 4 {
            return Err(Error::parse(format!(
                "superblock too short ({} bytes); need at least {}",
                bytes.len(),
                SUPERBLOCK_SECTOR_COUNT_OFFSET + 4
            )));
        }
        if !bytes.starts_with(VENDOR_CSTR) {
            return Err(Error::not_dave(format!(
                "missing vendor string {:?} at offset 0",
                String::from_utf8_lossy(&VENDOR_CSTR[..VENDOR_CSTR.len() - 1])
            )));
        }
        let magic_at = VENDOR_CSTR.len();
        if bytes.len() < magic_at + DISK_MAGIC_CSTR.len()
            || &bytes[magic_at..magic_at + DISK_MAGIC_CSTR.len()] != DISK_MAGIC_CSTR
        {
            return Err(Error::not_dave(
                "vendor string present but DaveSystemDisk magic missing",
            ));
        }

        let vendor = read_cstr(bytes, 0)?;
        let magic = read_cstr(bytes, magic_at)?;
        let sector_count = u32::from_le_bytes(
            bytes[SUPERBLOCK_SECTOR_COUNT_OFFSET..SUPERBLOCK_SECTOR_COUNT_OFFSET + 4]
                .try_into()
                .expect("slice length 4"),
        );

        let take = bytes.len().min(SUPERBLOCK_READ_LEN);
        Ok(Self {
            vendor,
            magic,
            sector_count,
            raw: bytes[..take].to_vec(),
        })
    }

    pub fn looks_like(bytes: &[u8]) -> bool {
        Self::parse(bytes).is_ok()
    }

    /// `sector_count * 512`, saturating.
    pub fn declared_size_bytes(&self) -> u64 {
        u64::from(self.sector_count).saturating_mul(SECTOR_SIZE)
    }
}
