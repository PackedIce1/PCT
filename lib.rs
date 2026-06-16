//! `pct` — Percent-encoding and decoding for URLs.
//!
//! A pure-Rust, zero-dependency crate with:
//!
//! - **Optional allocation** (`alloc` feature, on by default). Disable
//!   default features to use only the allocation-free core (`EncodeSet`,
//!   `is_valid`, scanning, length pre-computation) — useful in kernels
//!   and embedded targets without a heap.
//! - **Optional SIMD acceleration** (`simd` feature, requires nightly
//!   Rust via `#![feature(portable_simd)]`). Accelerates the no-op
//!   fast path so already-canonical inputs are scanned at 32 bytes per
//!   cycle on AVX2 / NEON targets.
//! - **`Cow`-based zero-allocation API** — when scanning determines no
//!   encoding/decoding is needed, the input is returned as
//!   `Cow::Borrowed` without ever touching the heap.
//! - **Issue #503 fixed** — bare `%` is encoded as `%25` by default
//!   (idempotent encoding skips already-valid `%XX` sequences).
//! - **Issues #416 / #482 fixed** — `+` is properly encoded as `%2B` in
//!   URL mode and decoded from form data via dedicated
//!   [`encode_form()`] / [`decode_form()`] functions.
//! - **Built-in `application/x-www-form-urlencoded`** — see the
//!   `form` module.
//! - **Arbitrary binary data** — [`encode_bytes()`] works on `&[u8]`.
//! - **Predefined context sets** — [`COMPONENT`](EncodeSet::COMPONENT),
//!   [`PATH`](EncodeSet::PATH), [`QUERY`](EncodeSet::QUERY),
//!   [`FRAGMENT`](EncodeSet::FRAGMENT) so you don't have to read the
//!   spec yourself.
//! - **Multiple decode strategies** — lossy, strict, and passthrough.
//! - **Normalization** — canonical form with uppercase hex and decoded
//!   unreserved characters.
//! - **Validation** — quick check for well-formed percent-encoding.
//!
//! # Quick start
//!
//! ```
//! use pct::{encode, decode, encode_form, decode_form};
//!
//! // URL percent-encoding (idempotent)
//! assert_eq!(encode("hello world"), "hello%20world");
//! assert_eq!(encode("100%"), "100%25");           // bare % encoded
//! assert_eq!(encode("foo%20bar"), "foo%20bar");    // already encoded → no-op
//!
//! // URL percent-decoding
//! assert_eq!(decode("hello%20world"), "hello world");
//!
//! // Form-urlencoded (space → +, + → %2B)
//! assert_eq!(encode_form("hello world"), "hello+world");
//! assert_eq!(decode_form("hello+world"), "hello world");
//! ```
//!
//! # `no_std` without `alloc`
//!
//! For environments without a heap (kernels, microcontrollers, boot
//! loaders), disable default features:
//!
//! ```toml
//! [dependencies]
//! pct = { version = "0.2", default-features = false }
//! ```
//!
//! The following APIs remain available without `alloc`:
//!
//! - [`EncodeSet`] and all predefined constants (`COMPONENT`, `PATH`, …)
//! - [`is_hex()`], [`hex_val()`], [`HEX_UPPER`], [`HEX_LOWER`]
//! - [`is_valid()`]
//! - [`find_first_byte()`], [`find_first_byte_raw()`],
//!   [`find_first_byte_idempotent()`]
//! - [`needs_encoding_raw()`], [`needs_encoding_idempotent()`]
//! - [`encoded_len_raw()`], [`encoded_len_idempotent()`]
//!
//! # SIMD acceleration
//!
//! Enable the `simd` feature on nightly Rust:
//!
//! ```toml
//! [dependencies]
//! pct = { version = "0.2", features = ["simd"] }
//! ```
//!
//! This enables `#![feature(portable_simd)]` internally and dispatches
//! the no-op fast path to `core::simd`-accelerated implementations.
//! Already-canonical inputs (the common case for valid URLs) are
//! scanned 32 bytes per cycle on AVX2 / NEON targets, bringing the
//! no-op cost close to the ~1.4 ns achieved by the `percent-encoding`
//! crate.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(feature = "simd", feature(portable_simd))]
#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs, clippy::doc_markdown)]

// `alloc` is only needed when the `alloc` (or `std`) feature is enabled.
// Without it, only the scanning/validation/EncodeSet APIs are available.
#[cfg(any(feature = "alloc", feature = "std"))]
extern crate alloc;

// ── Always-available modules ────────────────────────────────────────

mod hex;
mod scan;
mod set;

#[cfg(feature = "simd")]
mod simd;

// ── Alloc-gated modules ─────────────────────────────────────────────

#[cfg(any(feature = "alloc", feature = "std"))]
mod decode;
#[cfg(any(feature = "alloc", feature = "std"))]
mod encode;
#[cfg(any(feature = "alloc", feature = "std"))]
mod form;
#[cfg(any(feature = "alloc", feature = "std"))]
mod normalize;

#[cfg(all(any(feature = "alloc", feature = "std"), feature = "iri"))]
mod iri;

// ── Re-exports: always available ────────────────────────────────────

pub use hex::{decode_hex_pair, hex_val, is_hex, is_hex_lower, HEX_LOWER, HEX_UPPER};
pub use scan::{
    encoded_len_idempotent, encoded_len_raw, find_first_byte, find_first_byte_idempotent,
    find_first_byte_raw, is_valid, is_valid_bytes, needs_encoding_idempotent, needs_encoding_raw,
};
pub use set::EncodeSet;

// ── Re-exports: alloc-gated ─────────────────────────────────────────

#[cfg(any(feature = "alloc", feature = "std"))]
pub use decode::{decode, decode_bytes, decode_passthrough, decode_strict, DecodeError};
#[cfg(any(feature = "alloc", feature = "std"))]
pub use encode::{encode, encode_bytes, encode_raw, encode_with};
#[cfg(any(feature = "alloc", feature = "std"))]
pub use form::{decode_form, encode_form, encode_form_bytes};
#[cfg(any(feature = "alloc", feature = "std"))]
pub use normalize::normalize;

#[cfg(all(any(feature = "alloc", feature = "std"), feature = "iri"))]
pub use iri::encode_iri;

// ── Alloc-gated convenience APIs ────────────────────────────────────

#[cfg(any(feature = "alloc", feature = "std"))]
use alloc::borrow::Cow;
#[cfg(any(feature = "alloc", feature = "std"))]
use core::fmt;

#[cfg(any(feature = "alloc", feature = "std"))]
/// Percent-encode a string for a URL **path segment**.
///
/// Uses the [`PATH`](EncodeSet::PATH) set (keeps `/` unencoded) with
/// idempotent behaviour.
pub fn encode_for_path(input: &str) -> Cow<'_, str> {
    encode_with(input, &EncodeSet::PATH)
}

#[cfg(any(feature = "alloc", feature = "std"))]
/// Percent-encode a string for a URL **query string** (full string).
///
/// Uses the [`QUERY`](EncodeSet::QUERY) set (keeps `?`, `=`, `&`
/// unencoded) with idempotent behaviour. For individual query parameter
/// *values*, use [`encode()`] instead.
pub fn encode_for_query(input: &str) -> Cow<'_, str> {
    encode_with(input, &EncodeSet::QUERY)
}

#[cfg(any(feature = "alloc", feature = "std"))]
/// Percent-encode a string for a URL **fragment**.
///
/// Uses the [`FRAGMENT`](EncodeSet::FRAGMENT) set with idempotent
/// behaviour.
pub fn encode_for_fragment(input: &str) -> Cow<'_, str> {
    encode_with(input, &EncodeSet::FRAGMENT)
}

#[cfg(any(feature = "alloc", feature = "std"))]
/// Percent-encode a string for an isolated URL **component**.
///
/// This is an alias for [`encode()`] — uses the
/// [`COMPONENT`](EncodeSet::COMPONENT) set which encodes everything
/// except RFC 3986 unreserved characters.
pub fn encode_for_component(input: &str) -> Cow<'_, str> {
    encode(input)
}

// ── Display wrapper ────────────────────────────────────────────────

#[cfg(any(feature = "alloc", feature = "std"))]
/// A wrapper that percent-encodes a string when formatted with
/// [`Display`](fmt::Display).
///
/// Useful for inline use in `format!()` / `println!()`.
///
/// # Examples
///
/// ```
/// use pct::Encoded;
///
/// let url = format!("https://example.com/{}", Encoded("foo/bar"));
/// assert_eq!(url, "https://example.com/foo%2Fbar");
/// ```
pub struct Encoded<'a>(pub &'a str);

#[cfg(any(feature = "alloc", feature = "std"))]
impl fmt::Display for Encoded<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let encoded = encode(self.0);
        f.write_str(&encoded)
    }
}

// ── Trait ──────────────────────────────────────────────────────────

#[cfg(any(feature = "alloc", feature = "std"))]
/// Extension trait for percent-encoding/decoding on `&str`.
///
/// # Examples
///
/// ```
/// use pct::PercentEncode;
///
/// assert_eq!("hello world".percent_encode(), "hello%20world");
/// assert_eq!("hello%20world".percent_decode(), "hello world");
/// ```
pub trait PercentEncode {
    /// Percent-encode this string (idempotent, COMPONENT set).
    fn percent_encode(&self) -> Cow<'_, str>;

    /// Percent-decode this string (lossy).
    fn percent_decode(&self) -> Cow<'_, str>;
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl PercentEncode for str {
    fn percent_encode(&self) -> Cow<'_, str> {
        encode(self)
    }

    fn percent_decode(&self) -> Cow<'_, str> {
        decode(self)
    }
}

// ── Integration tests ──────────────────────────────────────────────

#[cfg(all(test, any(feature = "alloc", feature = "std")))]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn convenience_path() {
        assert_eq!(encode_for_path("a/b c"), "a/b%20c");
    }

    #[test]
    fn convenience_query() {
        assert_eq!(encode_for_query("k=v&x=1 y"), "k=v&x=1%20y");
    }

    #[test]
    fn convenience_fragment() {
        assert_eq!(encode_for_fragment("a#b c"), "a#b%20c");
    }

    #[test]
    fn encoded_wrapper() {
        let url = format!("https://example.com/{}", Encoded("foo/bar baz"));
        assert_eq!(url, "https://example.com/foo%2Fbar%20baz");
    }

    #[test]
    fn trait_on_str() {
        assert_eq!("hello world".percent_encode(), "hello%20world");
        assert_eq!("hello%20world".percent_decode(), "hello world");
    }

    // ── Issue #503 regression test ─────────────────────────────

    #[test]
    fn issue_503_percent_is_encoded() {
        // The original percent-encoding crate does NOT encode bare %
        assert_eq!(encode("100%"), "100%25");
        assert_eq!(encode("%"), "%25");
    }

    #[test]
    fn issue_503_idempotent() {
        // Already-encoded sequences are preserved
        assert_eq!(encode("foo%20bar"), "foo%20bar");
        // Calling encode twice produces the same result
        let first = encode("100% sure");
        let second = encode(&first);
        assert_eq!(first, second);
    }

    // ── Issues #416 / #482 regression tests ────────────────────

    #[test]
    fn issues_416_482_plus_in_url() {
        // In URL encoding, + is encoded as %2B (not left bare)
        assert_eq!(encode("a+b"), "a%2Bb");
    }

    #[test]
    fn issues_416_482_plus_in_form() {
        // In form encoding, + means space, literal + is %2B
        assert_eq!(encode_form("a+b"), "a%2Bb");
        assert_eq!(decode_form("a+b"), "a b");
        assert_eq!(decode_form("a%2Bb"), "a+b");
    }

    // ── Full round-trip tests ──────────────────────────────────

    #[test]
    fn roundtrip_url_encoding() {
        let original = "hello world!@#$%^&*()";
        let encoded = encode(original);
        let decoded = decode(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn roundtrip_form_encoding() {
        let original = "name=John Doe&age=30+";
        let encoded = encode_form(original);
        let decoded = decode_form(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn roundtrip_binary_data() {
        let data: &[u8] = &[0x00, 0x01, 0x20, 0x7F, 0x80, 0xFF];
        let encoded = encode_bytes(data, &EncodeSet::COMPONENT);
        let decoded = decode_bytes(&encoded);
        assert_eq!(&*decoded, data);
    }

    // ── no-alloc core API still works with alloc enabled ──────

    #[test]
    fn scan_apis_work() {
        let set = EncodeSet::COMPONENT;
        assert!(!needs_encoding_raw(b"hello", &set));
        assert!(needs_encoding_raw(b"hello world", &set));
        assert!(!needs_encoding_idempotent(b"foo%20bar", &set));
        assert!(needs_encoding_idempotent(b"100%", &set));

        assert_eq!(encoded_len_raw(b"hello world", &set, false), 5 + 3 + 5);
        assert_eq!(encoded_len_idempotent(b"foo%20bar", &set), 9);
    }
}
