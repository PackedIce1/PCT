//! Percent-encoding functions.
//!
//! Requires the `alloc` feature. The allocation-free scanning helpers
//! (`needs_encoding_raw`, `needs_encoding_idempotent`, `encoded_len_*`)
//! live in the [`crate::scan`] module and are always available.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use crate::hex::{hex_val, is_hex, HEX_UPPER};
use crate::scan::{
    encoded_len_idempotent, encoded_len_raw, find_first_byte_idempotent, find_first_byte_raw,
};
use crate::set::EncodeSet;

/// Percent-encode a string using the [`COMPONENT`](EncodeSet::COMPONENT) set
/// with **idempotent** behaviour.
///
/// Already-encoded sequences (`%XX` where `X` is a hex digit) are left as-is
/// and normalised to uppercase. Bare `%` characters not followed by two hex
/// digits are encoded as `%25`.
///
/// This is the safest default for encoding user-supplied text that may
/// already contain some percent-encoded sequences.
///
/// # Zero-allocation fast path
///
/// If the input needs no encoding, this function returns `Cow::Borrowed`
/// without allocating. The scan is SIMD-accelerated when the `simd` feature
/// is enabled.
///
/// # Examples
///
/// ```
/// use pct::encode;
///
/// assert_eq!(encode("hello world"), "hello%20world");
/// assert_eq!(encode("foo%20bar"), "foo%20bar"); // already encoded → no change
/// assert_eq!(encode("100%"), "100%25");         // bare % → encoded
/// ```
pub fn encode(input: &str) -> Cow<'_, str> {
    encode_with(input, &EncodeSet::COMPONENT)
}

/// Percent-encode a string with a custom [`EncodeSet`] and **idempotent**
/// behaviour.
///
/// See [`encode()`] for the idempotency rules.
pub fn encode_with<'a>(input: &'a str, set: &EncodeSet) -> Cow<'a, str> {
    let bytes = input.as_bytes();
    if find_first_byte_idempotent(bytes, set).is_none() {
        return Cow::Borrowed(input);
    }
    Cow::Owned(do_encode_idempotent(bytes, set))
}

/// Percent-encode a string with a custom [`EncodeSet`] in **raw** (non-idempotent) mode.
///
/// Every byte in the set is encoded, including `%`. Use this when you *know*
/// the input has not been previously percent-encoded.
///
/// # Examples
///
/// ```
/// use pct::{encode_raw, EncodeSet};
///
/// assert_eq!(encode_raw("hello%20world", &EncodeSet::COMPONENT), "hello%2520world");
/// ```
pub fn encode_raw<'a>(input: &'a str, set: &EncodeSet) -> Cow<'a, str> {
    let bytes = input.as_bytes();
    if find_first_byte_raw(bytes, set).is_none() {
        return Cow::Borrowed(input);
    }
    Cow::Owned(do_encode_raw(bytes, set, false))
}

/// Percent-encode arbitrary binary data with a custom [`EncodeSet`].
///
/// Always operates in raw (non-idempotent) mode. Bytes `0x80–0xFF` are
/// always encoded regardless of the set, ensuring the output is valid
/// ASCII suitable for URLs.
pub fn encode_bytes(input: &[u8], set: &EncodeSet) -> String {
    do_encode_raw(input, set, true)
}

// ── Internal helpers ────────────────────────────────────────────────

/// Idempotent encoding pass. Already-valid `%XX` sequences are preserved
/// (with hex digits normalised to uppercase); bare `%` is encoded as `%25`.
fn do_encode_idempotent(input: &[u8], set: &EncodeSet) -> String {
    // Pre-size the buffer using the scan module's length computation.
    // This avoids re-allocations during the encoding pass.
    let cap = encoded_len_idempotent(input, set);
    let mut out = Vec::with_capacity(cap);

    let mut i = 0;
    let len = input.len();
    while i < len {
        let byte = input[i];
        if byte == b'%' {
            if i + 2 < len && is_hex(input[i + 1]) && is_hex(input[i + 2]) {
                out.push(b'%');
                out.push(HEX_UPPER[hex_val(input[i + 1]) as usize]);
                out.push(HEX_UPPER[hex_val(input[i + 2]) as usize]);
                i += 3;
                continue;
            }
            out.extend_from_slice(b"%25");
            i += 1;
            continue;
        }
        if set.contains(byte) {
            out.push(b'%');
            out.push(HEX_UPPER[(byte >> 4) as usize]);
            out.push(HEX_UPPER[(byte & 0x0F) as usize]);
            i += 1;
        } else {
            out.push(byte);
            i += 1;
        }
    }
    // Safety: percent-encoding always produces ASCII; unencoded bytes are
    // copied from a valid UTF-8 input.
    unsafe { String::from_utf8_unchecked(out) }
}

/// Raw encoding pass. Every byte in the set (and optionally every byte ≥ 0x80)
/// is percent-encoded.
fn do_encode_raw(input: &[u8], set: &EncodeSet, force_high: bool) -> String {
    let cap = encoded_len_raw(input, set, force_high);
    let mut out = Vec::with_capacity(cap);
    for &byte in input {
        if set.contains(byte) || (force_high && byte >= 0x80) {
            out.push(b'%');
            out.push(HEX_UPPER[(byte >> 4) as usize]);
            out.push(HEX_UPPER[(byte & 0x0F) as usize]);
        } else {
            out.push(byte);
        }
    }
    // Safety: same reasoning as do_encode_idempotent — output is ASCII.
    unsafe { String::from_utf8_unchecked(out) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::borrow::Cow;

    // ── encode() ────────────────────────────────────────────────

    #[test]
    fn encode_noop() {
        let input = "hello";
        let result = encode(input);
        assert_eq!(result, "hello");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn encode_space() {
        assert_eq!(encode("hello world"), "hello%20world");
    }

    #[test]
    fn encode_fixes_issue_503_bare_percent() {
        // Bare % must be encoded as %25
        assert_eq!(encode("100%"), "100%25");
        assert_eq!(encode("50% off!"), "50%25%20off%21");
    }

    #[test]
    fn encode_idempotent_skips_valid_pct() {
        // Already-encoded sequences are preserved
        assert_eq!(encode("foo%20bar"), "foo%20bar");
        assert_eq!(encode("a%2Fb"), "a%2Fb");
    }

    #[test]
    fn encode_idempotent_normalises_hex_case() {
        // Lowercase hex is normalised to uppercase
        assert_eq!(encode("foo%2fbar"), "foo%2Fbar");
    }

    #[test]
    fn encode_idempotent_mixed() {
        // Mix of bare % and already-encoded
        assert_eq!(encode("100%25%20done"), "100%25%20done");
    }

    #[test]
    fn encode_reserved_chars() {
        assert_eq!(encode("a/b?c#d"), "a%2Fb%3Fc%23d");
    }

    #[test]
    fn encode_unreserved_preserved() {
        let input = "ABCxyz0123456789-._~";
        let result = encode(input);
        assert_eq!(result, input);
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn encode_long_unreserved_input_noop() {
        // A long all-unreserved input should hit the SIMD fast path and
        // return Cow::Borrowed without allocating.
        let input = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~\
                     ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
        let result = encode(input);
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(&*result, input);
    }

    // ── encode_raw() ────────────────────────────────────────────

    #[test]
    fn encode_raw_encodes_percent() {
        assert_eq!(
            encode_raw("foo%20bar", &EncodeSet::COMPONENT),
            "foo%2520bar"
        );
    }

    // ── encode_bytes() ──────────────────────────────────────────

    #[test]
    fn encode_bytes_binary() {
        let data: &[u8] = &[0x00, 0x01, 0xFF];
        let set = EncodeSet::COMPONENT;
        assert_eq!(encode_bytes(data, &set), "%00%01%FF");
    }

    #[test]
    fn encode_bytes_forces_high_bytes() {
        // Even with CONTROLS (minimal set), high bytes get encoded
        let data: &[u8] = b"hello\xC0\x80";
        assert_eq!(encode_bytes(data, &EncodeSet::CONTROLS), "hello%C0%80");
    }

    // ── encode_with() custom sets ───────────────────────────────

    #[test]
    fn encode_with_path_set() {
        assert_eq!(encode_with("a/b c", &EncodeSet::PATH), "a/b%20c");
    }

    #[test]
    fn encode_with_query_set() {
        assert_eq!(
            encode_with("key=val&x=1", &EncodeSet::QUERY),
            "key=val&x=1" // delimiters preserved
        );
    }

    // ── UTF-8 round-trip ────────────────────────────────────────

    #[test]
    fn encode_utf8() {
        assert_eq!(encode("café"), "caf%C3%A9");
    }

    #[test]
    fn encode_bytes_utf8_bytes() {
        let data = "café".as_bytes();
        assert_eq!(encode_bytes(data, &EncodeSet::COMPONENT), "caf%C3%A9");
    }
}
