use crate::constants::SECTOR_SIZE;

/// A single MBR partition table entry (DOS / Windows 95 style).
#[derive(Debug, Clone, Copy)]
pub struct MbrPartition {
    pub index: usize,
    pub bootable: bool,
    pub partition_type: u8,
    pub start_lba: u32,
    pub sector_count: u32,
}

impl MbrPartition {
    pub fn start_bytes(&self) -> u64 {
        u64::from(self.start_lba).saturating_mul(SECTOR_SIZE)
    }

    pub fn size_bytes(&self) -> u64 {
        u64::from(self.sector_count).saturating_mul(SECTOR_SIZE)
    }

    /// Linux native (0x83). The large Dave partition on ViP-class disks uses this type.
    pub fn is_linux(&self) -> bool {
        self.partition_type == 0x83
    }

    pub fn is_empty(&self) -> bool {
        self.partition_type == 0 || self.sector_count == 0
    }
}

/// Parsed protective / classic MBR. Only the four primary slots are read.
#[derive(Debug, Clone)]
pub struct Mbr {
    pub partitions: Vec<MbrPartition>,
}

impl Mbr {
    pub fn parse(sector: &[u8]) -> Option<Self> {
        if sector.len() < 512 {
            return None;
        }
        if sector[0x1FE] != 0x55 || sector[0x1FF] != 0xAA {
            return None;
        }

        let mut partitions = Vec::new();
        for index in 0..4 {
            let off = 0x1BE + index * 16;
            let entry = &sector[off..off + 16];
            let partition_type = entry[4];
            let start_lba = u32::from_le_bytes(entry[8..12].try_into().ok()?);
            let sector_count = u32::from_le_bytes(entry[12..16].try_into().ok()?);
            let part = MbrPartition {
                index,
                bootable: entry[0] == 0x80,
                partition_type,
                start_lba,
                sector_count,
            };
            if !part.is_empty() {
                partitions.push(part);
            }
        }
        Some(Self { partitions })
    }
}
