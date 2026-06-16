//! Percent-encoding normalization and validation.
//!
//! [`normalize()`] requires the `alloc` feature because it may produce
//! an owned `String` when the input is not already in canonical form.
//!
//! [`is_valid()`] lives in the [`crate::scan`] module and is always
//! available, even without `alloc`.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use crate::hex::{hex_val, is_hex, HEX_UPPER};

// `is_valid` lives in the `scan` module (always available, even without
// `alloc`) and is re-exported from the crate root. It is also used by
// the tests in this module.
#[cfg(test)]
use crate::scan::is_valid;

/// Normalise a percent-encoded string to its canonical form.
///
/// 1. Hex digits in `%XX` sequences are uppercased (`%2f` → `%2F`).
/// 2. Percent-encoded **unreserved** characters are decoded
///    (`%7E` → `~`, `%41` → `A`, etc.), per RFC 3986 §2.3 which
///    states that unreserved characters should never be encoded.
///
/// Returns `Cow::Borrowed` when the input is already in canonical form.
///
/// # Examples
///
/// ```
/// use pct::normalize;
///
/// assert_eq!(normalize("%2f%2F"), "%2F%2F");     // uppercase hex
/// assert_eq!(normalize("%7E"), "~");              // decode unreserved
/// assert_eq!(normalize("%41%42%43"), "ABC");      // decode unreserved
/// assert_eq!(normalize("hello%20world"), "hello%20world"); // space stays encoded
/// ```
pub fn normalize(input: &str) -> Cow<'_, str> {
    let bytes = input.as_bytes();
    if !needs_normalization(bytes) {
        return Cow::Borrowed(input);
    }
    Cow::Owned(do_normalize(bytes))
}

// ── Internal helpers ───────────────────────────────────────────────

/// Check whether the input needs any normalisation:
///   - lowercase hex digits in %XX
///   - encoded unreserved characters
fn needs_normalization(input: &[u8]) -> bool {
    let mut i = 0;
    let len = input.len();
    while i < len {
        if input[i] == b'%' {
            if i + 2 < len && is_hex(input[i + 1]) && is_hex(input[i + 2]) {
                // Check for lowercase hex
                if input[i + 1].is_ascii_lowercase() || input[i + 2].is_ascii_lowercase() {
                    return true;
                }
                // Check for encoded unreserved character
                let decoded = (hex_val(input[i + 1]) << 4) | hex_val(input[i + 2]);
                if is_unreserved(decoded) {
                    return true;
                }
                i += 3;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    false
}

fn do_normalize(input: &[u8]) -> String {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    let len = input.len();
    while i < len {
        if input[i] == b'%' && i + 2 < len && is_hex(input[i + 1]) && is_hex(input[i + 2]) {
            let decoded = (hex_val(input[i + 1]) << 4) | hex_val(input[i + 2]);
            if is_unreserved(decoded) {
                out.push(decoded);
            } else {
                out.push(b'%');
                out.push(HEX_UPPER[(decoded >> 4) as usize]);
                out.push(HEX_UPPER[(decoded & 0x0F) as usize]);
            }
            i += 3;
        } else {
            out.push(input[i]);
            i += 1;
        }
    }
    // Safety: normalisation only decodes valid ASCII unreserved chars or
    // uppercases hex digits, so the output is valid UTF-8.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_noop() {
        let input = "hello%20world";
        let result = normalize(input);
        assert_eq!(result, "hello%20world");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn normalize_uppercase_hex() {
        assert_eq!(normalize("%2f"), "%2F");
        assert_eq!(normalize("%2f%2F"), "%2F%2F");
    }

    #[test]
    fn normalize_decode_unreserved() {
        assert_eq!(normalize("%7E"), "~");
        assert_eq!(normalize("%41%42%43"), "ABC");
        assert_eq!(normalize("%61%62%63"), "abc");
        assert_eq!(normalize("%30%31%32"), "012");
        assert_eq!(normalize("%2D"), "-");
        assert_eq!(normalize("%2E"), ".");
        assert_eq!(normalize("%5F"), "_");
    }

    #[test]
    fn normalize_keep_reserved_encoded() {
        assert_eq!(normalize("%20"), "%20"); // space stays
        assert_eq!(normalize("%2F"), "%2F"); // / stays
        assert_eq!(normalize("%3F"), "%3F"); // ? stays
    }

    #[test]
    fn is_valid_true() {
        assert!(is_valid("hello"));
        assert!(is_valid("hello%20world"));
        assert!(is_valid("%C3%A9"));
        assert!(is_valid("%FF%00"));
    }

    #[test]
    fn is_valid_false() {
        assert!(!is_valid("hello%GG"));
        assert!(!is_valid("hello%2"));
        assert!(!is_valid("hello%"));
        assert!(!is_valid("%G0"));
    }
}
