//! IRI → URI encoding (internationalized resource identifiers).
//!
//! Requires both the `alloc` and `iri` features. An IRI can contain
//! non-ASCII characters directly (e.g. `café`); this module
//! percent-encodes the UTF-8 bytes of any non-ASCII character to produce
//! a valid URI that only contains ASCII.

use alloc::string::String;
use alloc::vec::Vec;

use crate::hex::HEX_UPPER;

/// Encode an IRI (Internationalized Resource Identifier) to a valid URI.
///
/// An IRI can contain non-ASCII characters directly (e.g. `café`). This
/// function percent-encodes the UTF-8 bytes of any non-ASCII character,
/// producing a valid URI that only contains ASCII.
///
/// ASCII characters that are not RFC 3986 unreserved characters are also
/// percent-encoded, using the same rules as [`crate::encode()`].
///
/// This function requires the `iri` feature to be enabled.
///
/// # Examples
///
/// ```
/// use pct::encode_iri;
///
/// assert_eq!(encode_iri("café"), "caf%C3%A9");
/// assert_eq!(encode_iri("hello world"), "hello%20world");
/// assert_eq!(encode_iri("日本語"), "%E6%97%A5%E6%9C%AC%E8%AA%9E");
/// assert_eq!(encode_iri("abc123-._~"), "abc123-._~");
/// ```
pub fn encode_iri(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3);
    let mut i = 0;
    let len = bytes.len();
    while i < len {
        let byte = bytes[i];
        if byte < 0x80 {
            // ASCII byte
            if is_unreserved(byte) {
                out.push(byte);
            } else {
                out.push(b'%');
                out.push(HEX_UPPER[(byte >> 4) as usize]);
                out.push(HEX_UPPER[(byte & 0x0F) as usize]);
            }
            i += 1;
        } else {
            // Non-ASCII: encode all bytes of the UTF-8 sequence
            let seq_len = utf8_seq_len(byte);
            for j in 0..seq_len {
                if i + j < len {
                    let b = bytes[i + j];
                    out.push(b'%');
                    out.push(HEX_UPPER[(b >> 4) as usize]);
                    out.push(HEX_UPPER[(b & 0x0F) as usize]);
                }
            }
            i += seq_len;
        }
    }
    // Safety: output is all ASCII (unreserved chars or %XX sequences)
    unsafe { String::from_utf8_unchecked(out) }
}

/// RFC 3986 unreserved characters: A-Z, a-z, 0-9, '-', '.', '_', '~'.
#[inline]
fn is_unreserved(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~'
    )
}

/// Determine the length of a UTF-8 sequence from its leading byte.
#[inline]
fn utf8_seq_len(leading_byte: u8) -> usize {
    match leading_byte {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1, // Invalid UTF-8 leading byte — treat as single byte
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iri_ascii_unreserved() {
        assert_eq!(encode_iri("abc123-._~"), "abc123-._~");
    }

    #[test]
    fn iri_ascii_reserved() {
        assert_eq!(encode_iri("hello world"), "hello%20world");
        assert_eq!(encode_iri("a/b"), "a%2Fb");
        assert_eq!(encode_iri("?#&"), "%3F%23%26");
    }

    #[test]
    fn iri_non_ascii_latin() {
        assert_eq!(encode_iri("café"), "caf%C3%A9");
    }

    #[test]
    fn iri_cjk() {
        assert_eq!(encode_iri("日本語"), "%E6%97%A5%E6%9C%AC%E8%AA%9E");
    }

    #[test]
    fn iri_emoji() {
        // 🎉 = F0 9F 8E 89
        assert_eq!(encode_iri("🎉"), "%F0%9F%8E%89");
    }

    #[test]
    fn iri_mixed() {
        assert_eq!(
            encode_iri("hello café/日本"),
            "hello%20caf%C3%A9%2F%E6%97%A5%E6%9C%AC"
        );
    }
}
