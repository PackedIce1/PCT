//! Hex-digit constants and helpers.
//!
//! These are always available, even when the `alloc` feature is disabled,
//! because they only operate on single bytes and have no allocation needs.
//!
//! They are the foundation of both encoding (byte → `%XX`) and decoding
//! (`%XX` → byte) and are reused by every other module.

/// Lookup table for uppercase hex digits: `0123456789ABCDEF`.
///
/// Used when *producing* percent-encoded output. Index with the high or
/// low nibble of a byte: `HEX_UPPER[(byte >> 4) as usize]`,
/// `HEX_UPPER[(byte & 0x0F) as usize]`.
pub const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";

/// Lookup table for lowercase hex digits: `0123456789abcdef`.
///
/// Provided for callers that need to produce lowercase percent-encoding
/// (e.g. some URL canonicalization schemes).
pub const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";

/// Returns `true` if `byte` is an ASCII hex digit (`0-9`, `a-f`, `A-F`).
#[inline]
pub const fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

/// Returns `true` if `byte` is a *lowercase* ASCII hex digit (`0-9`, `a-f`).
#[inline]
pub const fn is_hex_lower(byte: u8) -> bool {
    matches!(byte, b'0'..=b'9' | b'a'..=b'f')
}

/// Returns the numeric value of an ASCII hex digit, or `0` for non-hex
/// bytes.
///
/// Callers should check [`is_hex()`] first if they need to distinguish
/// "invalid" from "value 0".
#[inline]
pub const fn hex_val(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

/// Decode a `%XX` sequence given the two hex digits.
///
/// Returns the decoded byte value. Does *not* validate the digits —
/// callers should ensure `is_hex(hi) && is_hex(lo)` beforehand.
#[inline]
pub const fn decode_hex_pair(hi: u8, lo: u8) -> u8 {
    (hex_val(hi) << 4) | hex_val(lo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_upper_table() {
        assert_eq!(HEX_UPPER[0], b'0');
        assert_eq!(HEX_UPPER[10], b'A');
        assert_eq!(HEX_UPPER[15], b'F');
    }

    #[test]
    fn hex_lower_table() {
        assert_eq!(HEX_LOWER[10], b'a');
        assert_eq!(HEX_LOWER[15], b'f');
    }

    #[test]
    fn is_hex_digits() {
        assert!(is_hex(b'0'));
        assert!(is_hex(b'9'));
        assert!(is_hex(b'a'));
        assert!(is_hex(b'f'));
        assert!(is_hex(b'A'));
        assert!(is_hex(b'F'));
        assert!(!is_hex(b'g'));
        assert!(!is_hex(b'G'));
        assert!(!is_hex(b' '));
        assert!(!is_hex(b'%'));
    }

    #[test]
    fn hex_val_correct() {
        assert_eq!(hex_val(b'0'), 0);
        assert_eq!(hex_val(b'9'), 9);
        assert_eq!(hex_val(b'a'), 10);
        assert_eq!(hex_val(b'f'), 15);
        assert_eq!(hex_val(b'A'), 10);
        assert_eq!(hex_val(b'F'), 15);
        assert_eq!(hex_val(b'g'), 0); // invalid → 0
    }

    #[test]
    fn decode_hex_pair_correct() {
        assert_eq!(decode_hex_pair(b'2', b'0'), 0x20); // space
        assert_eq!(decode_hex_pair(b'F', b'F'), 0xFF);
        assert_eq!(decode_hex_pair(b'0', b'0'), 0x00);
        assert_eq!(decode_hex_pair(b'C', b'3'), 0xC3); // UTF-8 leading byte
    }
}
