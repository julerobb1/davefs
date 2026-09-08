use crate::constants::{
    BLOCK_SIZE_CANDIDATE, MPEG_PES_VIDEO_START, SUBSTORE_HEADER_LEN, SUBSTORE_MAGIC_CSTR,
    VIP_SUBSTORE_HINTS,
};
use crate::error::{Error, Result};
use crate::util::read_cstr;

/// Forum / name mapping for the three well-known inner stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubstoreRole {
    /// `AV_REQ_HD` — recordings.
    Recordings,
    /// `ES_RESERVED` — VOD (622/722 lore).
    Vod,
    /// `EFSMisc` — misc (may pair with ext `MISC_HD` / `catalog.cat`).
    Misc,
    Unknown,
}

impl SubstoreRole {
    pub fn from_name(name: &str) -> Self {
        match name {
            "AV_REQ_HD" => Self::Recordings,
            "ES_RESERVED" => Self::Vod,
            "EFSMisc" => Self::Misc,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recordings => "Recordings",
            Self::Vod => "VOD",
            Self::Misc => "Misc",
            Self::Unknown => "Unknown",
        }
    }
}

/// Parsed inner-substore header (`Long Live Dave!` + name).
#[derive(Debug, Clone)]
pub struct Substore {
    /// Byte offset from the start of the Dave partition.
    pub offset: u64,
    pub name: String,
    pub role: SubstoreRole,
    /// First little-endian u32 equal to [`BLOCK_SIZE_CANDIDATE`] in the header
    /// after the name, if any. **Hypothesis — do not treat as confirmed.**
    pub block_size_candidate: Option<u32>,
    /// Offset of that candidate relative to the substore start.
    pub block_size_candidate_at: Option<usize>,
    /// True if `00 00 01 E0` appears in the bytes immediately after the header
    /// (reference drive: `AV_REQ_HD`).
    pub mpeg_pes_e0_after_header: bool,
    pub raw_header: Vec<u8>,
    pub note: Option<String>,
}

impl Substore {
    pub fn parse(offset: u64, header: &[u8], peek_after: &[u8]) -> Result<Self> {
        if header.len() < SUBSTORE_MAGIC_CSTR.len() + 2 {
            return Err(Error::parse("substore header too short"));
        }
        if !header.starts_with(SUBSTORE_MAGIC_CSTR) {
            return Err(Error::parse("missing Long Live Dave! magic"));
        }

        let name_at = SUBSTORE_MAGIC_CSTR.len();
        let name = read_cstr(header, name_at)?;
        if name.is_empty() {
            return Err(Error::parse("empty substore name"));
        }

        let name_end = name_at + name.len() + 1;
        let (block_size_candidate, block_size_candidate_at) =
            find_block_size_candidate(header, name_end);

        let role = SubstoreRole::from_name(&name);
        let note = VIP_SUBSTORE_HINTS
            .iter()
            .find(|h| h.name == name)
            .map(|h| h.note.to_string());

        let take = header.len().min(SUBSTORE_HEADER_LEN);
        Ok(Self {
            offset,
            name,
            role,
            block_size_candidate,
            block_size_candidate_at,
            mpeg_pes_e0_after_header: find_pes(peek_after),
            raw_header: header[..take].to_vec(),
            note,
        })
    }

    pub fn looks_like(bytes: &[u8]) -> bool {
        bytes.starts_with(SUBSTORE_MAGIC_CSTR)
    }
}

fn find_block_size_candidate(header: &[u8], start: usize) -> (Option<u32>, Option<usize>) {
    let aligned = (start + 3) & !3;
    let mut off = aligned;
    while off + 4 <= header.len() {
        let value = u32::from_le_bytes(header[off..off + 4].try_into().expect("4"));
        if value == BLOCK_SIZE_CANDIDATE {
            return (Some(value), Some(off));
        }
        off += 4;
    }
    (None, None)
}

fn find_pes(bytes: &[u8]) -> bool {
    bytes
        .windows(MPEG_PES_VIDEO_START.len())
        .any(|w| w == MPEG_PES_VIDEO_START)
}
