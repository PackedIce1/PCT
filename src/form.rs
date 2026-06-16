//! `application/x-www-form-urlencoded` encoding and decoding.
//!
//! Requires the `alloc` feature. Form-encoding is semantically distinct
//! from URL percent-encoding: spaces map to `+` rather than `%20`, and
//! literal `+` characters are encoded as `%2B` so they can be
//! distinguished from spaces on decode.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use crate::hex::{hex_val, is_hex, HEX_UPPER};
use crate::scan::find_first_byte;
use crate::set::EncodeSet;

// ── Form-URL-Encoded set ───────────────────────────────────────────
// Per the WHATWG URL Standard / HTML §17.5.2, the
// application/x-www-form-urlencoded byte set is similar to COMPONENT
// but also explicitly encodes +, !, ', (, ), and * beyond the basic
// set. In practice COMPONENT already covers these. The key semantic
// difference is that SPACE maps to `+` instead of `%20`.

const FORM_SET: EncodeSet = EncodeSet::COMPONENT;

/// Encode a string for `application/x-www-form-urlencoded`.
///
/// Spaces become `+`, and all other characters in the form encode set
/// are percent-encoded. Literal `+` characters in the input are encoded
/// as `%2B` so they can be distinguished from spaces on decode.
///
/// This directly addresses the `+`-as-space ambiguity that
/// `percent-encoding` crate issues #416 and #482 complain about.
///
/// # Examples
///
/// ```
/// use pct::encode_form;
///
/// assert_eq!(encode_form("hello world"), "hello+world");
/// assert_eq!(encode_form("a+b"), "a%2Bb");       // literal + → %2B
/// assert_eq!(encode_form("key=val&x=1"), "key%3Dval%26x%3D1");
/// ```
pub fn encode_form(input: &str) -> String {
    let bytes = input.as_bytes();
    // Pre-compute an upper bound on the output size — every input byte
    // expands to at most 3 output bytes.
    let mut out = Vec::with_capacity(bytes.len() * 3);
    for &byte in bytes {
        if byte == b' ' {
            out.push(b'+');
        } else if FORM_SET.contains(byte) {
            out.push(b'%');
            out.push(HEX_UPPER[(byte >> 4) as usize]);
            out.push(HEX_UPPER[(byte & 0x0F) as usize]);
        } else {
            out.push(byte);
        }
    }
    // Safety: output is ASCII-only (percent-encoded or original ASCII bytes).
    unsafe { String::from_utf8_unchecked(out) }
}

/// Encode arbitrary binary data for `application/x-www-form-urlencoded`.
///
/// Same rules as [`encode_form()`], but bytes `0x80–0xFF` are always
/// percent-encoded.
pub fn encode_form_bytes(input: &[u8]) -> String {
    let mut out = Vec::with_capacity(input.len() * 3);
    for &byte in input {
        if byte == b' ' {
            out.push(b'+');
        } else if FORM_SET.contains(byte) || byte >= 0x80 {
            out.push(b'%');
            out.push(HEX_UPPER[(byte >> 4) as usize]);
            out.push(HEX_UPPER[(byte & 0x0F) as usize]);
        } else {
            out.push(byte);
        }
    }
    unsafe { String::from_utf8_unchecked(out) }
}

/// Decode an `application/x-www-form-urlencoded` string.
///
/// `+` is decoded as a space, and `%XX` sequences are decoded to their
/// byte values. Invalid percent-sequences are passed through as-is
/// (lossy behaviour).
///
/// # Zero-allocation fast path
///
/// If the input contains neither `+` nor `%`, returns `Cow::Borrowed`
/// without allocating. The `%` scan is SIMD-accelerated when the `simd`
/// feature is enabled.
///
/// # Examples
///
/// ```
/// use pct::decode_form;
///
/// assert_eq!(decode_form("hello+world"), "hello world");
/// assert_eq!(decode_form("a%2Bb"), "a+b");
/// ```
pub fn decode_form(input: &str) -> Cow<'_, str> {
    let bytes = input.as_bytes();

    // Quick check: does the input need any decoding?
    // We need to scan for either '+' or '%'. SIMD accelerates the '%'
    // scan; the '+' scan is fast enough scalar (slice::contains gets
    // auto-vectorized by the compiler).
    let has_pct = find_first_byte(bytes, b'%').is_some();
    let has_plus = bytes.contains(&b'+');
    if !has_pct && !has_plus {
        return Cow::Borrowed(input);
    }

    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let len = bytes.len();
    while i < len {
        if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if bytes[i] == b'%' {
            if i + 2 < len && is_hex(bytes[i + 1]) && is_hex(bytes[i + 2]) {
                let byte = (hex_val(bytes[i + 1]) << 4) | hex_val(bytes[i + 2]);
                out.push(byte);
                i += 3;
            } else {
                // Invalid sequence: pass through the %
                out.push(b'%');
                i += 1;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }

    // Validate UTF-8 — form data should be UTF-8, but be safe
    match String::from_utf8(out) {
        Ok(s) => Cow::Owned(s),
        Err(e) => {
            let lossy = String::from_utf8_lossy(e.as_bytes());
            Cow::Owned(lossy.into_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_encode_space_to_plus() {
        assert_eq!(encode_form("hello world"), "hello+world");
    }

    #[test]
    fn form_encode_plus_to_pct() {
        // Issue #416 / #482 fix: literal + must be distinguishable from space
        assert_eq!(encode_form("a+b"), "a%2Bb");
    }

    #[test]
    fn form_encode_special_chars() {
        assert_eq!(encode_form("key=val&x=1"), "key%3Dval%26x%3D1");
    }

    #[test]
    fn form_decode_plus_to_space() {
        assert_eq!(decode_form("hello+world"), "hello world");
    }

    #[test]
    fn form_decode_pct2b_to_plus() {
        assert_eq!(decode_form("a%2Bb"), "a+b");
    }

    #[test]
    fn form_roundtrip() {
        let original = "hello world+test=foo&bar";
        let encoded = encode_form(original);
        let decoded = decode_form(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn form_decode_noop() {
        let input = "hello";
        let result = decode_form(input);
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn form_encode_bytes() {
        let data: &[u8] = b"hello\xC3\xA9";
        assert_eq!(encode_form_bytes(data), "hello%C3%A9");
    }

    #[test]
    fn form_roundtrip_special_chars() {
        let original = "100% real + fresh & free";
        let encoded = encode_form(original);
        let decoded = decode_form(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn form_decode_long_no_op() {
        // Long input with no + or % should hit the fast path.
        let input = "abcdefghijklmnopqrstuvwxyz0123456789\
                     abcdefghijklmnopqrstuvwxyz0123456789\
                     abcdefghijklmnopqrstuvwxyz0123456789";
        let result = decode_form(input);
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(&*result, input);
    }
}
