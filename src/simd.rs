//! SIMD-accelerated byte scanning using `core::simd`.
//!
//! **Requires nightly Rust** and the `simd` crate feature, which enables
//! `#![feature(portable_simd)]`.
//!
//! # What's accelerated?
//!
//! The hot path for percent-encoding is the **no-op scan**: "does this
//! input need any encoding at all?" If the answer is no, the input is
//! returned as `Cow::Borrowed` without any allocation. This is the case
//! that matters most for performance — `percent-encoding` achieves its
//! famous ~1.4 ns latency here by doing essentially zero work.
//!
//! This module accelerates that scan using `core::simd`:
//!
//! - [`find_first_byte_simd()`] scans 32 bytes per cycle looking for a
//!   specific byte (e.g. `%`). Used by `decode()` to fast-path inputs
//!   with no percent-encoding.
//! - [`find_first_byte_idempotent_simd()`] uses an "all unreserved ASCII"
//!   check to skip 32-byte chunks that are guaranteed to need no encoding.
//!   Only "non-clean" chunks fall back to the scalar path.
//! - [`find_first_byte_raw_simd()`] does the same for raw encoding mode.
//!
//! # Why 32-byte lanes?
//!
//! `u8x32` (256 bits) matches AVX2 on x86-64 and is two NEON lanes on
//! ARM64. The `core::simd` abstractions lower to native instructions on
//! both, with no runtime detection needed — the compiler emits the best
//! code for the target CPU.

// Pull in the SIMD comparison traits. On nightly Rust these live in
// `core::simd::prelude` (re-exported from `core::simd::cmp`).
use core::simd::prelude::*;
use core::simd::{u8x16, u8x32};

use crate::scan::find_first_byte_idempotent_scalar;
use crate::set::EncodeSet;

/// Returns `true` if every byte in the chunk is an RFC 3986 *unreserved*
/// character: `A-Z`, `a-z`, `0-9`, or one of `-`, `.`, `_`, `~`.
///
/// Such bytes never need encoding (in any predefined set) and never need
/// percent-decoding, so a chunk where this returns `true` can be skipped
/// entirely. This is the key SIMD fast-path check.
#[inline]
fn all_unreserved(chunk: u8x32) -> bool {
    // Range checks (each comparison is one SIMD instruction on AVX2).
    let is_upper = chunk.simd_ge(u8x32::splat(b'A')) & chunk.simd_le(u8x32::splat(b'Z'));
    let is_lower = chunk.simd_ge(u8x32::splat(b'a')) & chunk.simd_le(u8x32::splat(b'z'));
    let is_digit = chunk.simd_ge(u8x32::splat(b'0')) & chunk.simd_le(u8x32::splat(b'9'));

    // Equality checks for the four unreserved punctuation bytes.
    let is_dash = chunk.simd_eq(u8x32::splat(b'-'));
    let is_dot = chunk.simd_eq(u8x32::splat(b'.'));
    let is_under = chunk.simd_eq(u8x32::splat(b'_'));
    let is_tilde = chunk.simd_eq(u8x32::splat(b'~'));

    // OR everything together — if any lane is *not* set, the byte is not
    // unreserved and the chunk needs scalar inspection.
    (is_upper | is_lower | is_digit | is_dash | is_dot | is_under | is_tilde).all()
}

/// SIMD scan for the first occurrence of `byte` in `input`.
///
/// Processes 32 bytes per cycle. Returns the byte index, or `None` if not
/// present. Used by `decode()` to fast-path inputs with no `%`.
#[inline]
pub fn find_first_byte_simd(input: &[u8], byte: u8) -> Option<usize> {
    let needle = u8x32::splat(byte);
    let mut i = 0;
    let len = input.len();

    // 32-byte SIMD lanes
    while i + 32 <= len {
        // SAFETY: `from_slice` requires `slice.len() >= 32`, guaranteed by
        // the loop bound. It performs an unaligned load when necessary.
        let chunk = u8x32::from_slice(&input[i..i + 32]);
        // `simd_eq` returns `Mask<i8, 32>` for `u8x32` — let inference
        // handle it rather than annotating (u8 is not a MaskElement).
        let mask = chunk.simd_eq(needle);
        if let Some(lane) = mask.first_set() {
            return Some(i + lane);
        }
        i += 32;
    }

    // 16-byte tail lane (if present)
    if i + 16 <= len {
        let chunk = u8x16_from_slice(&input[i..i + 16]);
        let mask = chunk.simd_eq(u8x16::splat(byte));
        if let Some(lane) = mask.first_set() {
            return Some(i + lane);
        }
        i += 16;
    }

    // Scalar tail
    input[i..].iter().position(|&b| b == byte).map(|p| i + p)
}

/// SIMD scan for the first byte needing encoding in **idempotent** mode.
///
/// Strategy: scan 32-byte chunks with [`all_unreserved()`]. If a chunk
/// is fully unreserved, skip it. As soon as a non-clean chunk is found,
/// fall back to the scalar implementation starting from that chunk's
/// position — the scalar code will correctly handle `%XX` sequences,
/// bare `%`, lowercase-hex normalization, and set membership.
#[inline]
pub fn find_first_byte_idempotent_simd(input: &[u8], set: &EncodeSet) -> Option<usize> {
    let mut i = 0;
    let len = input.len();

    while i + 32 <= len {
        let chunk = u8x32::from_slice(&input[i..i + 32]);
        if all_unreserved(chunk) {
            // Whole chunk is guaranteed clean — skip.
            i += 32;
            continue;
        }
        // Non-clean chunk found — break to scalar scan from current position.
        break;
    }

    // Scalar scan from current position (handles %XX logic correctly).
    find_first_byte_idempotent_scalar(&input[i..], set).map(|p| i + p)
}

/// SIMD scan for the first byte needing encoding in **raw** mode.
///
/// Uses the same `all_unreserved` fast path. For raw mode this is slightly
/// conservative — the FRAGMENT set, for instance, leaves `#` unencoded, so
/// a chunk containing only `#` and unreserved bytes would be flagged as
/// "non-clean" even though no encoding is needed. The scalar fallback
/// then resolves the exact answer correctly. The conservative behavior
/// is acceptable because the SIMD scan is only an optimization — it
/// never produces wrong answers, only occasionally falls back to scalar
/// earlier than strictly necessary.
#[inline]
pub fn find_first_byte_raw_simd(input: &[u8], set: &EncodeSet) -> Option<usize> {
    let mut i = 0;
    let len = input.len();

    while i + 32 <= len {
        let chunk = u8x32::from_slice(&input[i..i + 32]);
        if all_unreserved(chunk) {
            i += 32;
            continue;
        }
        break;
    }

    // Scalar fallback. We don't have a `find_first_byte_raw_scalar` that
    // takes an offset, so we slice and remap.
    input[i..]
        .iter()
        .position(|&b| set.contains(b))
        .map(|p| i + p)
}

// ── 16-byte helper ──────────────────────────────────────────────────
//
// `u8x16` is already imported at the top of this file via
// `use core::simd::{Mask, u8x16, u8x32}`. The wrapper below is a thin
// alias used by the 16-byte tail lane in `find_first_byte_simd`.

#[inline]
fn u8x16_from_slice(slice: &[u8]) -> u8x16 {
    u8x16::from_slice(slice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simd_find_first_byte_present() {
        let input = b"hello world this is a test %20";
        // Index: 0='h' 5=' ' 27='%' (count: hello=5, space=1, world=5,
        // space=1, this=4, space=1, is=2, space=1, a=1, space=1, test=4,
        // space=1, %=1 → 5+1+5+1+4+1+2+1+1+1+4+1 = 27)
        assert_eq!(find_first_byte_simd(input, b'%'), Some(27));
        assert_eq!(find_first_byte_simd(input, b' '), Some(5));
    }

    #[test]
    fn simd_find_first_byte_absent() {
        let input = b"abcdefghijklmnopqrstuvwxyz012345"; // 32 bytes
        assert_eq!(find_first_byte_simd(input, b' '), None);
        assert_eq!(find_first_byte_simd(input, b'%'), None);
    }

    #[test]
    fn simd_find_first_byte_short_input() {
        // Inputs shorter than 32 bytes exercise the tail path.
        assert_eq!(find_first_byte_simd(b"hi", b'i'), Some(1));
        assert_eq!(find_first_byte_simd(b"hi", b'z'), None);
    }

    #[test]
    fn simd_find_first_byte_at_lane_boundary() {
        // 32 'a's followed by '%' — exercises the SIMD path then the tail.
        let mut input = [b'a'; 33];
        input[32] = b'%';
        assert_eq!(find_first_byte_simd(&input, b'%'), Some(32));

        // '%' right at the start of the second chunk (offset 32).
        let mut input = [b'a'; 33];
        input[32] = b'%';
        assert_eq!(find_first_byte_simd(&input, b'%'), Some(32));

        // '%' inside the second chunk (offset 33).
        let mut input = [b'a'; 34];
        input[33] = b'%';
        assert_eq!(find_first_byte_simd(&input, b'%'), Some(33));
    }

    #[test]
    fn simd_idempotent_clean_chunk() {
        let set = EncodeSet::COMPONENT;
        // 32 unreserved bytes → fully SIMD-skipped
        let input = b"abcdefghijklmnopqrstuvwxyz012345";
        assert_eq!(find_first_byte_idempotent_simd(input, &set), None);
    }

    #[test]
    fn simd_idempotent_dirty_chunk() {
        let set = EncodeSet::COMPONENT;
        // A space in the middle of an otherwise-clean chunk
        let mut input = [b'a'; 32];
        input[10] = b' ';
        assert_eq!(find_first_byte_idempotent_simd(&input, &set), Some(10));
    }

    #[test]
    fn simd_idempotent_pct_sequence_preserved() {
        let set = EncodeSet::COMPONENT;
        // A valid %XX sequence is *not* "unreserved" (because of %), so
        // the SIMD scan will fall back to scalar — which correctly
        // recognizes it as already-encoded and returns None.
        let input = b"aaaaaaaaaaaaaaaaaaaaaaa%20aaa"; // 30 bytes
                                                      // This input is too short to hit the 32-byte SIMD lane; the
                                                      // scalar fallback handles it directly.
        assert_eq!(find_first_byte_idempotent_simd(input, &set), None);
    }

    #[test]
    fn simd_raw_clean_chunk() {
        let set = EncodeSet::COMPONENT;
        let input = b"abcdefghijklmnopqrstuvwxyz012345";
        assert_eq!(find_first_byte_raw_simd(input, &set), None);
    }

    #[test]
    fn simd_raw_dirty_chunk() {
        let set = EncodeSet::COMPONENT;
        let mut input = [b'a'; 32];
        input[20] = b'/';
        assert_eq!(find_first_byte_raw_simd(&input, &set), Some(20));
    }
}
