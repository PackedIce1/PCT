//! Allocation-free byte scanning for percent-encoding decisions.
//!
//! These functions power the **zero-allocation fast path**: when scanning
//! determines that no encoding is needed, the input can be returned as
//! `Cow::Borrowed` without ever touching the heap.
//!
//! All functions in this module are `const`-friendly, have no dependencies
//! on `alloc`, and work in `#![no_std]` environments. They are the
//! foundation that the `encode`, `decode`, `normalize`, and `form` modules
//! build on.
//!
//! # SIMD acceleration
//!
//! When the `simd` feature is enabled (nightly only), [`find_first_byte()`],
//! [`find_first_byte_idempotent()`], and [`find_first_byte_raw()`] dispatch
//! to `core::simd`-accelerated implementations that process 32 bytes per
//! cycle on AVX2 / NEON targets. The scalar fallback is always available.

use crate::hex::is_hex;
use crate::set::EncodeSet;

// ── Single-byte search ───────────────────────────────────────────────

/// Find the index of the first occurrence of `byte` in `input`, or `None`
/// if not present.
///
/// This is the primary fast-path check used by `decode()` — if there is no
/// `%` byte, the input is already decoded and can be returned as
/// `Cow::Borrowed`.
///
/// When the `simd` feature is enabled, this dispatches to a SIMD
/// implementation that scans 32 bytes per cycle.
#[inline]
pub fn find_first_byte(input: &[u8], byte: u8) -> Option<usize> {
    #[cfg(feature = "simd")]
    {
        crate::simd::find_first_byte_simd(input, byte)
    }
    #[cfg(not(feature = "simd"))]
    {
        find_first_byte_scalar(input, byte)
    }
}

/// Scalar fallback for [`find_first_byte()`].
///
/// Only called when the `simd` feature is disabled. When `simd` is
/// enabled, the SIMD version in `crate::simd` is used instead (it has
/// its own scalar tail for sub-32-byte remainders).
#[inline]
#[allow(dead_code)] // only used when `simd` feature is off
pub(crate) fn find_first_byte_scalar(input: &[u8], byte: u8) -> Option<usize> {
    input.iter().position(|&b| b == byte)
}

// ── Idempotent-mode scanning ─────────────────────────────────────────

/// Returns the index of the first byte that needs encoding in **idempotent**
/// mode, or `None` if the input is already canonical.
///
/// In idempotent mode:
/// - Already-valid `%XX` sequences with **uppercase** hex digits are
///   preserved (no encoding needed).
/// - `%XX` sequences with **lowercase** hex digits need normalization.
/// - Bare `%` (or `%` not followed by two hex digits) needs encoding.
/// - Any byte in `set` needs encoding.
///
/// This is the hot-path scan used by `encode()` and `encode_with()` to
/// decide whether to return `Cow::Borrowed`.
///
/// When the `simd` feature is enabled, the initial bulk of the input is
/// scanned with SIMD; only "non-clean" chunks fall back to scalar.
#[inline]
pub fn find_first_byte_idempotent(input: &[u8], set: &EncodeSet) -> Option<usize> {
    #[cfg(feature = "simd")]
    {
        crate::simd::find_first_byte_idempotent_simd(input, set)
    }
    #[cfg(not(feature = "simd"))]
    {
        find_first_byte_idempotent_scalar(input, set)
    }
}

/// Scalar implementation of [`find_first_byte_idempotent()`].
///
/// Exposed as `pub(crate)` so the `simd` module can call it for tail
/// processing and non-clean chunks.
#[inline]
pub(crate) fn find_first_byte_idempotent_scalar(input: &[u8], set: &EncodeSet) -> Option<usize> {
    let mut i = 0;
    let len = input.len();
    while i < len {
        let byte = input[i];
        if byte == b'%' {
            // Need at least 2 more bytes for a valid %XX
            if i + 2 < len && is_hex(input[i + 1]) && is_hex(input[i + 2]) {
                // Valid sequence — but if either digit is lowercase, we
                // need to normalize it to uppercase.
                if input[i + 1].is_ascii_lowercase() || input[i + 2].is_ascii_lowercase() {
                    return Some(i);
                }
                i += 3;
                continue;
            }
            // Bare % or truncated/invalid sequence → must encode.
            return Some(i);
        }
        if set.contains(byte) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Returns `true` if the input needs encoding (idempotent mode).
///
/// Convenience wrapper around [`find_first_byte_idempotent()`].
#[inline]
pub fn needs_encoding_idempotent(input: &[u8], set: &EncodeSet) -> bool {
    find_first_byte_idempotent(input, set).is_some()
}

// ── Raw-mode scanning ────────────────────────────────────────────────

/// Returns the index of the first byte in `set`, or `None` if the input
/// is already valid for the given set in raw (non-idempotent) mode.
///
/// This is the hot-path scan used by `encode_raw()` to decide whether to
/// return `Cow::Borrowed`.
///
/// When the `simd` feature is enabled, dispatches to a SIMD-accelerated
/// implementation.
#[inline]
pub fn find_first_byte_raw(input: &[u8], set: &EncodeSet) -> Option<usize> {
    #[cfg(feature = "simd")]
    {
        crate::simd::find_first_byte_raw_simd(input, set)
    }
    #[cfg(not(feature = "simd"))]
    {
        find_first_byte_raw_scalar(input, set)
    }
}

/// Scalar implementation of [`find_first_byte_raw()`].
///
/// Only called when the `simd` feature is disabled.
#[inline]
#[allow(dead_code)] // only used when `simd` feature is off
pub(crate) fn find_first_byte_raw_scalar(input: &[u8], set: &EncodeSet) -> Option<usize> {
    input.iter().position(|&b| set.contains(b))
}

/// Returns `true` if any byte in the input needs encoding (raw mode).
#[inline]
pub fn needs_encoding_raw(input: &[u8], set: &EncodeSet) -> bool {
    find_first_byte_raw(input, set).is_some()
}

// ── Length pre-computation ───────────────────────────────────────────

/// Compute the length of the encoded output for the given input in **raw**
/// mode, without allocating the output.
///
/// Useful for pre-sizing buffers in `no_std` environments where you want
/// to write into a fixed-size array rather than a `Vec`.
///
/// * `force_high` — if `true`, bytes `0x80–0xFF` are always counted as
///   3 bytes (`%XX`) regardless of the set.
pub fn encoded_len_raw(input: &[u8], set: &EncodeSet, force_high: bool) -> usize {
    let mut len = 0;
    for &b in input {
        if set.contains(b) || (force_high && b >= 0x80) {
            len += 3; // %XX
        } else {
            len += 1;
        }
    }
    len
}

/// Compute the length of the encoded output for the given input in
/// **idempotent** mode, without allocating the output.
///
/// Already-valid uppercase `%XX` sequences count as 3 bytes (preserved);
/// bare `%` counts as 3 bytes (`%25`); each byte in `set` counts as 3.
pub fn encoded_len_idempotent(input: &[u8], set: &EncodeSet) -> usize {
    let mut len = 0;
    let mut i = 0;
    let n = input.len();
    while i < n {
        let byte = input[i];
        if byte == b'%' {
            if i + 2 < n && is_hex(input[i + 1]) && is_hex(input[i + 2]) {
                // Valid sequence — preserved as 3 bytes (normalized to uppercase).
                len += 3;
                i += 3;
                continue;
            }
            // Bare % → %25
            len += 3;
            i += 1;
            continue;
        }
        if set.contains(byte) {
            len += 3;
        } else {
            len += 1;
        }
        i += 1;
    }
    len
}

// ── Validation ───────────────────────────────────────────────────────

/// Returns `true` if the input is valid percent-encoding.
///
/// Every `%` must be followed by two hex digits. No other validation
/// (e.g. UTF-8 correctness) is performed. This is allocation-free and
/// works in `#![no_std]` environments.
///
/// # Examples
///
/// ```
/// use pct::is_valid;
///
/// assert!(is_valid("hello%20world"));
/// assert!(!is_valid("hello%GG"));
/// assert!(!is_valid("hello%2"));
/// assert!(is_valid("no-encoding"));
/// ```
#[inline]
pub fn is_valid(input: &str) -> bool {
    is_valid_bytes(input.as_bytes())
}

/// Same as [`is_valid()`] but operates on raw bytes.
#[inline]
pub fn is_valid_bytes(input: &[u8]) -> bool {
    let mut i = 0;
    let len = input.len();
    while i < len {
        if input[i] == b'%' {
            if i + 2 >= len {
                return false;
            }
            if !is_hex(input[i + 1]) || !is_hex(input[i + 2]) {
                return false;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_first_byte_simple() {
        assert_eq!(find_first_byte(b"hello world", b' '), Some(5));
        assert_eq!(find_first_byte(b"hello", b' '), None);
        assert_eq!(find_first_byte(b"%20", b'%'), Some(0));
    }

    #[test]
    fn find_first_byte_idempotent_clean() {
        let set = EncodeSet::COMPONENT;
        assert_eq!(
            find_first_byte_idempotent(b"hello world", &set),
            Some(5) // space
        );
        assert_eq!(find_first_byte_idempotent(b"hello", &set), None);
    }

    #[test]
    fn find_first_byte_idempotent_preserves_pct() {
        let set = EncodeSet::COMPONENT;
        // Already-valid uppercase %XX → preserved, no scan hit
        assert_eq!(find_first_byte_idempotent(b"foo%20bar", &set), None);
    }

    #[test]
    fn find_first_byte_idempotent_lowercase_pct() {
        let set = EncodeSet::COMPONENT;
        // Lowercase %2f → must be normalized to %2F
        assert_eq!(find_first_byte_idempotent(b"foo%2fbar", &set), Some(3));
    }

    #[test]
    fn find_first_byte_idempotent_bare_pct() {
        let set = EncodeSet::COMPONENT;
        // Bare % at end → must encode
        assert_eq!(find_first_byte_idempotent(b"100%", &set), Some(3));
        // Bare % in middle
        assert_eq!(find_first_byte_idempotent(b"100% off", &set), Some(3));
    }

    #[test]
    fn find_first_byte_raw_basic() {
        let set = EncodeSet::COMPONENT;
        assert_eq!(find_first_byte_raw(b"hello", &set), None);
        assert_eq!(find_first_byte_raw(b"hello world", &set), Some(5));
        // In raw mode, % is itself in the set → flagged
        assert_eq!(find_first_byte_raw(b"foo%20bar", &set), Some(3));
    }

    #[test]
    fn needs_encoding_raw_check() {
        let set = EncodeSet::COMPONENT;
        assert!(!needs_encoding_raw(b"hello", &set));
        assert!(needs_encoding_raw(b"hello world", &set));
    }

    #[test]
    fn needs_encoding_idempotent_check() {
        let set = EncodeSet::COMPONENT;
        assert!(!needs_encoding_idempotent(b"hello", &set));
        assert!(!needs_encoding_idempotent(b"foo%20bar", &set));
        assert!(needs_encoding_idempotent(b"100%", &set));
        assert!(needs_encoding_idempotent(b"foo%2fbar", &set));
    }

    #[test]
    fn encoded_len_raw_basic() {
        // COMPONENT includes the 0x80-0xFF range, so high bytes count as
        // 3 regardless of `force_high`. Use CONTROLS (which does *not*
        // include high bytes) to exercise the force_high flag.
        let comp = EncodeSet::COMPONENT;
        let ctrl = EncodeSet::CONTROLS;

        // No encoding needed → length unchanged
        assert_eq!(encoded_len_raw(b"hello", &comp, false), 5);
        // One space → 3 bytes
        assert_eq!(encoded_len_raw(b"hello world", &comp, false), 5 + 3 + 5);

        // With CONTROLS set: 0xFF is *not* in the set.
        // force_high=false → byte passes through as 1 byte.
        assert_eq!(encoded_len_raw(&[0xFF], &ctrl, false), 1);
        // force_high=true  → byte is encoded as 3 bytes.
        assert_eq!(encoded_len_raw(&[0xFF], &ctrl, true), 3);

        // With COMPONENT set: 0xFF *is* in the set.
        // Either way → 3 bytes.
        assert_eq!(encoded_len_raw(&[0xFF], &comp, false), 3);
        assert_eq!(encoded_len_raw(&[0xFF], &comp, true), 3);
    }

    #[test]
    fn encoded_len_idempotent_preserves_pct() {
        let set = EncodeSet::COMPONENT;
        // %20 preserved as 3 bytes
        assert_eq!(encoded_len_idempotent(b"foo%20bar", &set), 9);
        // bare % → %25 (3 bytes)
        assert_eq!(encoded_len_idempotent(b"100%", &set), 6);
    }

    #[test]
    fn is_valid_works() {
        assert!(is_valid("hello%20world"));
        assert!(is_valid("%C3%A9"));
        assert!(is_valid("no-encoding"));
        assert!(!is_valid("hello%GG"));
        assert!(!is_valid("hello%2"));
        assert!(!is_valid("hello%"));
    }

    #[test]
    fn is_valid_bytes_works() {
        assert!(is_valid_bytes(b"%FF%00"));
        assert!(!is_valid_bytes(b"%G0"));
    }
}
