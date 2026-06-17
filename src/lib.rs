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
//! - **Compile-time encoding** — [`const_encode!`] performs percent-encoding
//!   at compile time with **zero runtime cost**. The encoded string is
//!   embedded directly in the binary.
//! - **Streaming/iterator API** — [`EncodedBytes`] and [`DecodedChars`]
//!   yield encoded bytes or decoded characters one at a time with **zero
//!   heap allocation**, enabling processing of massive inputs without
//!   loading them into RAM.
//! - **Explicit mode API** — [`Pct`] provides clearly-named methods for
//!   RFC 3986, WHATWG URL Standard, and HTML Form encoding, preventing
//!   bugs from applying the wrong type of encoding.
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
//!   [`FRAGMENT`](EncodeSet::FRAGMENT), [`WHATWG`](EncodeSet::WHATWG)
//!   so you don't have to read the spec yourself.
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
//! # Compile-time encoding
//!
//! Use [`const_encode!`] to encode strings at compile time with **zero
//! runtime cost**:
//!
//! ```
//! use pct::const_encode;
//!
//! const ENCODED: &str = const_encode!("Hello World");
//! assert_eq!(ENCODED, "Hello%20World");
//! ```
//!
//! # Streaming / zero-allocation
//!
//! Use [`EncodedBytes`] and [`DecodedChars`] to process data incrementally
//! without allocating:
//!
//! ```
//! use pct::{EncodedBytes, DecodedChars, EncodeSet};
//!
//! // Encode byte-by-byte
//! let mut encoder = EncodedBytes::new("hello world", &EncodeSet::COMPONENT);
//! let encoded: Vec<u8> = encoder.collect();
//!
//! // Decode char-by-char
//! let mut decoder = DecodedChars::new("hello%20world");
//! let decoded: String = decoder.collect();
//! ```
//!
//! # Explicit modes
//!
//! Use [`Pct`] to make the encoding standard unambiguous:
//!
//! ```
//! use pct::Pct;
//!
//! assert_eq!(Pct::encode_rfc3986("hello world"), "hello%20world");
//! assert_eq!(Pct::encode_whatwg("keep'safe"), "keep'safe");
//! assert_eq!(Pct::encode_html_form("hello world"), "hello+world");
//! ```
//!
//! # `no_std` without `alloc`
//!
//! For environments without a heap (kernels, microcontrollers, boot
//! loaders), disable default features:
//!
//! ```toml
//! [dependencies]
//! pct = { version = "0.3", default-features = false }
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
//! - [`EncodedBytes`], [`DecodedChars`] (streaming iterators)
//! - [`const_encode_to_buf()`], [`const_encoded_len()`] (const helpers)
//! - [`const_encode!`] (compile-time encoding macro)
//!
//! # SIMD acceleration
//!
//! Enable the `simd` feature on nightly Rust:
//!
//! ```toml
//! [dependencies]
//! pct = { version = "0.3", features = ["simd"] }
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

mod const_encode;
mod hex;
mod scan;
mod set;
mod stream;

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
mod modes;
#[cfg(any(feature = "alloc", feature = "std"))]
mod normalize;

#[cfg(all(any(feature = "alloc", feature = "std"), feature = "iri"))]
mod iri;

// ── Re-exports: always available ────────────────────────────────────

pub use const_encode::{
    const_encode_to_buf, const_encoded_len, CONST_ENCODE_BUF_SIZE, MAX_CONST_INPUT_LEN,
};
pub use hex::{decode_hex_pair, hex_val, is_hex, is_hex_lower, HEX_LOWER, HEX_UPPER};
pub use scan::{
    encoded_len_idempotent, encoded_len_raw, find_first_byte, find_first_byte_idempotent,
    find_first_byte_raw, is_valid, is_valid_bytes, needs_encoding_idempotent, needs_encoding_raw,
};
pub use set::EncodeSet;
pub use stream::{DecodedChars, EncodedBytes};

// ── Re-exports: alloc-gated ─────────────────────────────────────────

#[cfg(any(feature = "alloc", feature = "std"))]
pub use decode::{decode, decode_bytes, decode_passthrough, decode_strict, DecodeError};
#[cfg(any(feature = "alloc", feature = "std"))]
pub use encode::{encode, encode_bytes, encode_raw, encode_with};
#[cfg(any(feature = "alloc", feature = "std"))]
pub use form::{decode_form, encode_form, encode_form_bytes};
#[cfg(any(feature = "alloc", feature = "std"))]
pub use modes::Pct;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use normalize::normalize;

#[cfg(all(any(feature = "alloc", feature = "std"), feature = "iri"))]
pub use iri::encode_iri;

// ── Compile-time encoding macro ────────────────────────────────────

/// Percent-encode a string at **compile time**.
///
/// This macro performs percent-encoding during compilation, producing a
/// `&'static str` with **zero runtime cost**. The encoded string is
/// embedded directly in the binary as a constant.
///
/// This effectively makes the runtime cost **0 ns** for static strings —
/// a feature few percent-encoding libraries offer.
///
/// # Arguments
///
/// - `$input` — A string literal (or any `const` `&str` expression) to
///   encode.
/// - `$set` *(optional)* — A reference to an [`EncodeSet`]. Defaults to
///   [`COMPONENT`](EncodeSet::COMPONENT) (RFC 3986 unreserved characters).
///
/// # Limits
///
/// The input must be at most [`MAX_CONST_INPUT_LEN`] (1024) bytes.
/// Longer inputs will cause a compile-time error.
///
/// # Examples
///
/// ```
/// use pct::const_encode;
///
/// // Default: COMPONENT set (RFC 3986 unreserved chars)
/// const ENCODED: &str = const_encode!("Hello World");
/// assert_eq!(ENCODED, "Hello%20World");
///
/// // Custom encode set
/// const PATH_ENCODED: &str = const_encode!("a/b c", &pct::EncodeSet::PATH);
/// assert_eq!(PATH_ENCODED, "a/b%20c");
///
/// // Works in let bindings too (compile-time computed, zero runtime cost)
/// let encoded: &str = const_encode!("café");
/// assert_eq!(encoded, "caf%C3%A9");
/// ```
///
/// # Compile-time validation
///
/// If the input exceeds the maximum length, compilation fails with a
/// descriptive panic.
#[macro_export]
macro_rules! const_encode {
    ($input:expr) => {
        $crate::const_encode!($input, &$crate::EncodeSet::COMPONENT)
    };
    ($input:expr, $set:expr) => {{
        const INPUT: &str = $input;
        const SET: &$crate::EncodeSet = $set;
        const INPUT_BYTES: &[u8] = INPUT.as_bytes();

        // Compile-time length check.
        //
        // NOTE: `panic!` with format arguments is not const-stable on
        // stable Rust, so we use a plain literal message here. The exact
        // limit is [`MAX_CONST_INPUT_LEN`](crate::MAX_CONST_INPUT_LEN).
        const _: () = {
            if INPUT_BYTES.len() > $crate::MAX_CONST_INPUT_LEN {
                panic!("const_encode! input exceeds maximum length");
            }
        };

        const ENCODED_LEN: usize = $crate::const_encoded_len(INPUT_BYTES, SET);

        // Encode into a fixed-size array of exactly the encoded length.
        //
        // NOTE: We deliberately avoid slicing a max-sized buffer
        // (`buf[..ENCODED_LEN]`) because range indexing is not yet
        // const-stable on stable Rust — it requires the nightly-only
        // `const_index` feature. By making the array exactly
        // `ENCODED_LEN` bytes long, we can convert it to `&str` later
        // without any slicing.
        const BYTES: [u8; ENCODED_LEN] = {
            let mut buf = [0u8; ENCODED_LEN];
            let mut i = 0;
            let mut o = 0;
            while i < INPUT_BYTES.len() {
                let b = INPUT_BYTES[i];
                if SET.contains(b) {
                    buf[o] = b'%';
                    buf[o + 1] = $crate::HEX_UPPER[(b >> 4) as usize];
                    buf[o + 2] = $crate::HEX_UPPER[(b & 0x0F) as usize];
                    o += 3;
                } else {
                    buf[o] = b;
                    o += 1;
                }
                i += 1;
            }
            buf
        };

        // SAFETY: percent-encoding produces only ASCII bytes:
        //   1. Unencoded bytes are copied from valid UTF-8 input.
        //   2. Encoded bytes are `%XX` sequences, which are pure ASCII.
        // ASCII is a strict subset of UTF-8, so the output is always
        // valid UTF-8. We still call `from_utf8` so any encoder bug
        // becomes a compile error rather than UB.
        //
        // NOTE: `BYTES.as_slice()` (const-stable since Rust 1.83) avoids
        // the range-indexing operation that would otherwise require
        // `const_index`.
        const ENCODED: &str = match core::str::from_utf8(BYTES.as_slice()) {
            Ok(s) => s,
            Err(_) => {
                panic!("const_encode: internal error - output is not valid UTF-8")
            }
        };
        ENCODED
    }};
}
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
    use alloc::string::String;
    use alloc::vec::Vec;

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

    // ── const_encode! macro tests ──────────────────────────────

    #[test]
    fn const_encode_basic() {
        const ENCODED: &str = const_encode!("Hello World");
        assert_eq!(ENCODED, "Hello%20World");
    }

    #[test]
    fn const_encode_noop() {
        const ENCODED: &str = const_encode!("hello");
        assert_eq!(ENCODED, "hello");
    }

    #[test]
    fn const_encode_special_chars() {
        const ENCODED: &str = const_encode!("a/b?c#d");
        assert_eq!(ENCODED, "a%2Fb%3Fc%23d");
    }

    #[test]
    fn const_encode_utf8() {
        const ENCODED: &str = const_encode!("café");
        assert_eq!(ENCODED, "caf%C3%A9");
    }

    #[test]
    fn const_encode_path_set() {
        const ENCODED: &str = const_encode!("a/b c", &EncodeSet::PATH);
        assert_eq!(ENCODED, "a/b%20c");
    }

    #[test]
    fn const_encode_whatwg_set() {
        const ENCODED: &str = const_encode!("keep'safe", &EncodeSet::WHATWG);
        assert_eq!(ENCODED, "keep'safe");
    }

    #[test]
    fn const_encode_let_binding() {
        let encoded: &str = const_encode!("test 123");
        assert_eq!(encoded, "test%20123");
    }

    // ── Streaming iterator round-trip ──────────────────────────

    #[test]
    fn stream_roundtrip() {
        let original = "hello world! café";
        let set = EncodeSet::COMPONENT;

        // Encode via iterator
        let encoded_bytes: Vec<u8> = EncodedBytes::new(original, &set).collect();
        let encoded_str = String::from_utf8(encoded_bytes).unwrap();

        // Decode via iterator
        let decoded: String = DecodedChars::new(&encoded_str).collect();
        assert_eq!(decoded, original);
    }

    // ── Pct mode tests ─────────────────────────────────────────

    #[test]
    fn pct_modes_basic() {
        use crate::Pct;

        assert_eq!(Pct::encode_rfc3986("hello world"), "hello%20world");
        assert_eq!(Pct::encode_whatwg("keep'safe"), "keep'safe");
        assert_eq!(Pct::encode_html_form("hello world"), "hello+world");
    }
}
