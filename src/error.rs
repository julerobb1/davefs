use std::fmt;
use std::io;
use std::path::PathBuf;

/// Recoverable davefs errors. All I/O is read-only; none of these imply a write.
#[derive(Debug)]
pub enum Error {
    Io {
        path: Option<PathBuf>,
        source: io::Error,
    },
    NotDave {
        reason: String,
    },
    OffsetOutOfRange {
        offset: u64,
        length: u64,
    },
    Parse(String),
}

impl Error {
    pub fn io(path: impl Into<Option<PathBuf>>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn not_dave(reason: impl Into<String>) -> Self {
        Self::NotDave {
            reason: reason.into(),
        }
    }

    pub fn parse(msg: impl Into<String>) -> Self {
        Self::Parse(msg.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                path: Some(p),
                source,
            } => {
                write!(f, "read error on {}: {source}", p.display())
            }
            Self::Io { path: None, source } => write!(f, "read error: {source}"),
            Self::NotDave { reason } => write!(f, "not a DaveSystemDisk image: {reason}"),
            Self::OffsetOutOfRange { offset, length } => {
                write!(f, "offset {offset:#x} is past end of file ({length} bytes)")
            }
            Self::Parse(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(source: io::Error) -> Self {
        Self::io(None, source)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
