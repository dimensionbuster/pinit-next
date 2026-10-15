//! Minimal standard-alphabet base64 decoder (avoids a dependency for one call site).

fn val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

/// Decodes standard or URL-safe base64; padding optional, ASCII whitespace ignored.
pub fn decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in input.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | val(c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    // leftover bits must be zero padding, and a lone sextet is invalid
    (acc == 0 && bits < 6).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_with_and_without_padding() {
        assert_eq!(decode("TWFu").unwrap(), b"Man");
        assert_eq!(decode("TWE=").unwrap(), b"Ma");
        assert_eq!(decode("TWE").unwrap(), b"Ma");
        assert_eq!(decode("TQ==").unwrap(), b"M");
        assert_eq!(decode("").unwrap(), b"");
    }

    #[test]
    fn ignores_whitespace_and_accepts_url_safe() {
        assert_eq!(decode("TW\nFu").unwrap(), b"Man");
        assert_eq!(decode("-_8=").unwrap(), [0xFB, 0xFF]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode("TW!u").is_none());
        assert!(decode("T").is_none());
    }
}
