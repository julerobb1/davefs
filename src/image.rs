use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::constants::{
    MIB, SUBSTORE_HEADER_LEN, SUBSTORE_MAGIC_CSTR, SUPERBLOCK_READ_LEN, VIP_SUBSTORE_HINTS,
};
use crate::error::{Error, Result};
use crate::mbr::Mbr;
use crate::substore::Substore;
use crate::superblock::Superblock;

/// Images smaller than this are scanned in full for `Long Live Dave!` markers.
const SMALL_IMAGE_FULL_SCAN: u64 = 16 * MIB;

/// On large images, scan this many bytes from the partition start in addition
/// to the published ViP hint offsets. Avoids reading a 460 GiB dump by default.
const LARGE_IMAGE_PREFIX_SCAN: u64 = 128 * MIB;

const SCAN_CHUNK: usize = 1024 * 1024;

/// How the Dave partition was located inside the opened file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    /// File begins with the Dave superblock (a partition dump).
    PartitionImage,
    /// Whole-disk image; Dave lives at [`DaveImage::partition_offset`].
    WholeDisk,
    /// Caller supplied `--partition-offset`.
    ExplicitOffset,
}

/// Read-only handle to a DaveSystemDisk partition inside a file.
///
/// The underlying file is opened with write disabled. There is no API that
/// writes to the image.
#[derive(Debug)]
pub struct DaveImage {
    path: PathBuf,
    file: File,
    file_len: u64,
    partition_offset: u64,
    kind: ImageKind,
    superblock: Superblock,
}

impl DaveImage {
    /// Open `path` read-only. If `partition_offset` is `None`, auto-detect a
    /// Dave superblock at byte 0 or inside an MBR 0x83 partition.
    pub fn open(path: impl AsRef<Path>, partition_offset: Option<u64>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new()
            .read(true)
            .write(false)
            .create(false)
            .open(&path)
            .map_err(|e| Error::io(Some(path.clone()), e))?;
        let file_len = file
            .metadata()
            .map_err(|e| Error::io(Some(path.clone()), e))?
            .len();

        let (partition_offset, kind) = match partition_offset {
            Some(off) => (off, ImageKind::ExplicitOffset),
            None => detect_partition_offset(&mut file, file_len, &path)?,
        };

        if partition_offset >= file_len {
            return Err(Error::OffsetOutOfRange {
                offset: partition_offset,
                length: file_len,
            });
        }

        let mut header = vec![0u8; SUPERBLOCK_READ_LEN];
        let n = read_at(&mut file, partition_offset, &mut header)?;
        header.truncate(n);
        let superblock = Superblock::parse(&header)?;

        Ok(Self {
            path,
            file,
            file_len,
            partition_offset,
            kind,
            superblock,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file_len(&self) -> u64 {
        self.file_len
    }

    /// Byte offset of the Dave partition inside the opened file.
    pub fn partition_offset(&self) -> u64 {
        self.partition_offset
    }

    pub fn kind(&self) -> ImageKind {
        self.kind
    }

    pub fn superblock(&self) -> &Superblock {
        &self.superblock
    }

    /// Bytes readable from the Dave partition start to EOF.
    pub fn readable_partition_len(&self) -> u64 {
        self.file_len.saturating_sub(self.partition_offset)
    }

    /// Read `buf.len()` bytes at `offset` relative to the Dave partition start.
    pub fn read_partition(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        let abs = self
            .partition_offset
            .checked_add(offset)
            .ok_or_else(|| Error::parse("offset overflow"))?;
        if abs >= self.file_len {
            return Err(Error::OffsetOutOfRange {
                offset: abs,
                length: self.file_len,
            });
        }
        read_at(&mut self.file, abs, buf)
    }

    /// Read `buf.len()` bytes at an absolute file offset.
    pub fn read_file(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        if offset >= self.file_len {
            return Err(Error::OffsetOutOfRange {
                offset,
                length: self.file_len,
            });
        }
        read_at(&mut self.file, offset, buf)
    }

    /// Locate inner substores.
    ///
    /// Always checks the published ViP hint offsets (when they fall inside the
    /// file). Then scans either the whole readable partition (small images, or
    /// `full_scan`) or the first 128 MiB (large dumps).
    pub fn substores(&mut self, full_scan: bool) -> Result<Vec<Substore>> {
        let mut found: Vec<Substore> = Vec::new();

        for hint in VIP_SUBSTORE_HINTS {
            let off = hint.offset_bytes();
            if off >= self.readable_partition_len() {
                continue;
            }
            if let Some(ss) = self.try_substore(off)? {
                push_unique(&mut found, ss);
            }
        }

        let scan_end = if full_scan || self.readable_partition_len() <= SMALL_IMAGE_FULL_SCAN {
            self.readable_partition_len()
        } else {
            LARGE_IMAGE_PREFIX_SCAN.min(self.readable_partition_len())
        };
        self.scan_range(0, scan_end, &mut found)?;
        found.sort_by_key(|s| s.offset);
        Ok(found)
    }

    fn try_substore(&mut self, offset: u64) -> Result<Option<Substore>> {
        let mut header = vec![0u8; SUBSTORE_HEADER_LEN];
        let n = match self.read_partition(offset, &mut header) {
            Ok(n) => n,
            Err(Error::OffsetOutOfRange { .. }) => return Ok(None),
            Err(e) => return Err(e),
        };
        header.truncate(n);
        if !Substore::looks_like(&header) {
            return Ok(None);
        }
        let mut peek = vec![0u8; 256];
        let pn = self
            .read_partition(offset + header.len() as u64, &mut peek)
            .unwrap_or(0);
        peek.truncate(pn);
        match Substore::parse(offset, &header, &peek) {
            Ok(ss) => Ok(Some(ss)),
            Err(_) => Ok(None),
        }
    }

    fn scan_range(&mut self, start: u64, end: u64, found: &mut Vec<Substore>) -> Result<()> {
        if start >= end {
            return Ok(());
        }
        let magic = SUBSTORE_MAGIC_CSTR;
        let mut pos = start;
        let mut overlap = vec![0u8; 0];
        while pos < end {
            let want = SCAN_CHUNK.min((end - pos) as usize);
            let mut chunk = vec![0u8; want];
            let n = self.read_partition(pos, &mut chunk)?;
            chunk.truncate(n);
            if chunk.is_empty() {
                break;
            }

            let mut haystack = overlap.clone();
            haystack.extend_from_slice(&chunk);
            let hay_base = pos.saturating_sub(overlap.len() as u64);

            let mut search_from = 0;
            while let Some(rel) = find_bytes(&haystack[search_from..], magic) {
                let abs = hay_base + (search_from + rel) as u64;
                if let Some(ss) = self.try_substore(abs)? {
                    push_unique(found, ss);
                }
                search_from += rel + 1;
            }

            overlap = if chunk.len() >= magic.len() {
                chunk[chunk.len() - (magic.len() - 1)..].to_vec()
            } else {
                chunk
            };
            pos += n as u64;
        }
        Ok(())
    }
}

fn detect_partition_offset(
    file: &mut File,
    file_len: u64,
    path: &Path,
) -> Result<(u64, ImageKind)> {
    let mut probe = vec![0u8; SUPERBLOCK_READ_LEN.max(512)];
    let n = read_at(file, 0, &mut probe).map_err(|e| match e {
        Error::Io { source, .. } => Error::io(Some(path.to_path_buf()), source),
        other => other,
    })?;
    probe.truncate(n);

    if Superblock::looks_like(&probe) {
        return Ok((0, ImageKind::PartitionImage));
    }

    if let Some(mbr) = Mbr::parse(&probe) {
        let mut candidates: Vec<&crate::mbr::MbrPartition> =
            mbr.partitions.iter().filter(|p| p.is_linux()).collect();
        if candidates.is_empty() {
            candidates = mbr.partitions.iter().collect();
        }
        for part in candidates {
            let off = part.start_bytes();
            if off >= file_len {
                continue;
            }
            let mut hdr = vec![0u8; SUPERBLOCK_READ_LEN];
            if read_at(file, off, &mut hdr).is_err() {
                continue;
            }
            if Superblock::looks_like(&hdr) {
                return Ok((off, ImageKind::WholeDisk));
            }
        }
        return Err(Error::not_dave(
            "MBR present but no Linux 0x83 (or other) partition starts with DaveSystemDisk",
        ));
    }

    Err(Error::not_dave(
        "file does not start with Echostar/DaveSystemDisk and is not an MBR disk image",
    ))
}

fn read_at(file: &mut File, offset: u64, buf: &mut [u8]) -> Result<usize> {
    file.seek(SeekFrom::Start(offset))?;
    let n = file.read(buf)?;
    Ok(n)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn push_unique(found: &mut Vec<Substore>, ss: Substore) {
    if !found.iter().any(|e| e.offset == ss.offset) {
        found.push(ss);
    }
}
