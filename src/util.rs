use crate::error::{Error, Result};

/// Read a NUL-terminated ASCII/UTF-8 C-string starting at `at`.
pub fn read_cstr(buf: &[u8], at: usize) -> Result<String> {
    if at >= buf.len() {
        return Err(Error::parse("C-string starts past end of buffer"));
    }
    let end = buf[at..]
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| Error::parse("unterminated C-string"))?;
    let raw = &buf[at..at + end];
    if raw.iter().any(|b| !(0x20..=0x7E).contains(b)) {
        return Err(Error::parse(format!(
            "C-string at {at:#x} is not printable ASCII"
        )));
    }
    Ok(String::from_utf8_lossy(raw).into_owned())
}

/// Parse a byte count from CLI text: decimal, `0x` hex, optional `K`/`KiB`/`M`/`MiB`/`G`/`GiB`.
pub fn parse_bytes(s: &str) -> Result<u64> {
    let s = s.trim();
    if s.is_empty() {
        return Err(Error::parse("empty byte count"));
    }

    let (num, mul) = split_suffix(s)?;
    let value = if let Some(hex) = num.strip_prefix("0x").or_else(|| num.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|e| Error::parse(format!("invalid hex {num}: {e}")))?
    } else {
        num.parse::<u64>()
            .map_err(|e| Error::parse(format!("invalid integer {num}: {e}")))?
    };
    value
        .checked_mul(mul)
        .ok_or_else(|| Error::parse("byte count overflow"))
}

fn split_suffix(s: &str) -> Result<(&str, u64)> {
    const SUFFIXES: &[(&str, u64)] = &[
        ("GiB", 1024 * 1024 * 1024),
        ("MiB", 1024 * 1024),
        ("KiB", 1024),
        ("GB", 1000 * 1000 * 1000),
        ("MB", 1000 * 1000),
        ("KB", 1000),
        ("G", 1024 * 1024 * 1024),
        ("M", 1024 * 1024),
        ("K", 1024),
        ("g", 1024 * 1024 * 1024),
        ("m", 1024 * 1024),
        ("k", 1024),
    ];
    for (suf, mul) in SUFFIXES {
        if let Some(num) = s.strip_suffix(suf) {
            return Ok((num, *mul));
        }
    }
    Ok((s, 1))
}

/// Classic 16-byte hex + ASCII dump. `base` is the printed address of `data[0]`.
pub fn hexdump(data: &[u8], base: u64) -> String {
    let mut out = String::new();
    for (row, chunk) in data.chunks(16).enumerate() {
        let addr = base + (row as u64) * 16;
        out.push_str(&format!("{addr:08x}  "));
        for i in 0..16 {
            if i == 8 {
                out.push(' ');
            }
            if let Some(b) = chunk.get(i) {
                out.push_str(&format!("{b:02x} "));
            } else {
                out.push_str("   ");
            }
        }
        out.push_str(" |");
        for b in chunk {
            let c = if (0x20..=0x7E).contains(b) {
                *b as char
            } else {
                '.'
            };
            out.push(c);
        }
        out.push_str("|\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bytes_accepts_hex_and_suffixes() {
        assert_eq!(parse_bytes("256").unwrap(), 256);
        assert_eq!(parse_bytes("0x48").unwrap(), 0x48);
        assert_eq!(parse_bytes("64KiB").unwrap(), 64 * 1024);
        assert_eq!(parse_bytes("64M").unwrap(), 64 * 1024 * 1024);
    }
}
