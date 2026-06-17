//! Explicit encoding/decoding modes for different URL standards.
//!
//! The [`Pct`] struct provides a clear, mode-explicit API that prevents bugs
//! where users accidentally apply the wrong type of encoding. Instead of
//! remembering which function to call for which context, use:
//!
//! - [`Pct::encode_rfc3986()`] — strict RFC 3986 URI encoding (space → `%20`)
//! - [`Pct::encode_whatwg()`] — WHATWG URL Standard encoding (more permissive)
//! - [`Pct::encode_html_form()`] — `application/x-www-form-urlencoded` (space → `+`)
//!
//! Each mode also has a corresponding decode function.
//!
//! Requires the `alloc` feature.

use alloc::borrow::Cow;
use alloc::string::String;

use crate::decode::decode;
use crate::encode::encode_with;
use crate::form::{decode_form, encode_form};
use crate::set::EncodeSet;

/// Explicit encoding/decoding modes for different URL standards.
///
/// "URL encoding" is ambiguous — RFC 3986, the WHATWG URL Standard, and
/// HTML form encoding all have different rules. This struct provides
/// clearly-named methods for each standard, preventing bugs where users
/// apply the wrong type of encoding to a URL path vs a query parameter.
///
/// # When to use each mode
///
/// | Context | Method | Space | `+` sign |
/// |---------|--------|-------|----------|
/// | URL path / component | [`encode_rfc3986`](Self::encode_rfc3986) | `%20` | `%2B` |
/// | WHATWG URL parsing | [`encode_whatwg`](Self::encode_whatwg) | `%20` | passed through |
/// | HTML `<form>` submission | [`encode_html_form`](Self::encode_html_form) | `+` | `%2B` |
///
/// # Examples
///
/// ```
/// use pct::Pct;
///
/// // RFC 3986: strict, for URI components
/// assert_eq!(Pct::encode_rfc3986("hello world"), "hello%20world");
/// assert_eq!(Pct::encode_rfc3986("a+b"), "a%2Bb");
///
/// // WHATWG URL Standard: more permissive (allows !, ', (, ), *, etc.)
/// assert_eq!(Pct::encode_whatwg("hello world"), "hello%20world");
/// assert_eq!(Pct::encode_whatwg("keep'safe"), "keep'safe");
///
/// // HTML Form: space → +
/// assert_eq!(Pct::encode_html_form("hello world"), "hello+world");
/// assert_eq!(Pct::encode_html_form("a+b"), "a%2Bb");
/// ```
pub struct Pct;

impl Pct {
    // ── Encoding ──────────────────────────────────────────────────

    /// Encode a string using **strict RFC 3986** rules.
    ///
    /// Only RFC 3986 unreserved characters (`A-Z a-z 0-9 - . _ ~`) pass
    /// through unencoded. Everything else is percent-encoded with uppercase
    /// hex digits. Spaces become `%20`.
    ///
    /// This is the safest default for encoding an individual URL component
    /// value (e.g. a path segment, query parameter value, or fragment).
    ///
    /// Equivalent to [`crate::encode()`] (always allocates, returning
    /// `String`).
    ///
    /// # Examples
    ///
    /// ```
    /// use pct::Pct;
    ///
    /// assert_eq!(Pct::encode_rfc3986("hello world"), "hello%20world");
    /// assert_eq!(Pct::encode_rfc3986("café"), "caf%C3%A9");
    /// assert_eq!(Pct::encode_rfc3986("a+b"), "a%2Bb");
    /// ```
    pub fn encode_rfc3986(input: &str) -> String {
        encode_with(input, &EncodeSet::COMPONENT).into_owned()
    }

    /// Encode a string following the **WHATWG URL Standard**.
    ///
    /// The WHATWG standard defines "URL code points" — a broader set of
    /// allowed characters than RFC 3986 unreserved characters. Characters
    /// like `!`, `'`, `(`, `)`, `*`, `+`, `,`, `/`, `:`, `;`, `=`, `?`,
    /// `@` are allowed without encoding.
    ///
    /// This is more permissive than [`encode_rfc3986()`](Self::encode_rfc3986)
    /// and matches the behaviour of browsers' `URL` constructor when
    /// setting properties like `pathname`.
    ///
    /// # What gets encoded
    ///
    /// Only C0 controls, space, `"`, `#`, `<`, `>`, `\`, `^`, `` ` ``,
    /// `{`, `|`, `}`, DEL, and non-ASCII bytes are encoded. Everything
    /// else (all "URL code points") passes through.
    ///
    /// # Examples
    ///
    /// ```
    /// use pct::Pct;
    ///
    /// assert_eq!(Pct::encode_whatwg("hello world"), "hello%20world");
    /// assert_eq!(Pct::encode_whatwg("keep'safe"), "keep'safe");
    /// assert_eq!(Pct::encode_whatwg("a+b"), "a+b");
    /// assert_eq!(Pct::encode_whatwg("path/to/file"), "path/to/file");
    /// assert_eq!(Pct::encode_whatwg("price=100&sale"), "price=100&sale");
    /// ```
    pub fn encode_whatwg(input: &str) -> String {
        encode_with(input, &EncodeSet::WHATWG).into_owned()
    }

    /// Encode a string for **HTML form submission**
    /// (`application/x-www-form-urlencoded`).
    ///
    /// Spaces become `+` (not `%20`), and all other non-unreserved
    /// characters are percent-encoded. Literal `+` characters are encoded
    /// as `%2B` so they can be distinguished from spaces on decode.
    ///
    /// Equivalent to [`crate::encode_form()`].
    ///
    /// # Examples
    ///
    /// ```
    /// use pct::Pct;
    ///
    /// assert_eq!(Pct::encode_html_form("hello world"), "hello+world");
    /// assert_eq!(Pct::encode_html_form("a+b"), "a%2Bb");
    /// assert_eq!(Pct::encode_html_form("key=val&x=1"), "key%3Dval%26x%3D1");
    /// ```
    pub fn encode_html_form(input: &str) -> String {
        encode_form(input)
    }

    // ── Decoding ──────────────────────────────────────────────────

    /// Decode a **RFC 3986** percent-encoded string (lossy).
    ///
    /// Valid `%XX` sequences are decoded. Invalid sequences are replaced
    /// with `U+FFFD` (the Unicode replacement character).
    ///
    /// Equivalent to [`crate::decode()`].
    ///
    /// # Examples
    ///
    /// ```
    /// use pct::Pct;
    ///
    /// assert_eq!(Pct::decode_rfc3986("hello%20world"), "hello world");
    /// assert_eq!(Pct::decode_rfc3986("caf%C3%A9"), "café");
    /// ```
    pub fn decode_rfc3986(input: &str) -> Cow<'_, str> {
        decode(input)
    }

    /// Decode a string following the **WHATWG URL Standard** (lossy).
    ///
    /// Valid `%XX` sequences are decoded to bytes, which are interpreted
    /// as UTF-8. Invalid sequences are replaced with `U+FFFD`.
    ///
    /// The WHATWG standard's percent-decoding is compatible with RFC 3986
    /// for the common case. The main differences are at the URL parser
    /// level (handling of delimiters like `#`, `?`), not at the encoding
    /// level. For basic decoding, this behaves identically to
    /// [`decode_rfc3986()`](Self::decode_rfc3986).
    ///
    /// # Examples
    ///
    /// ```
    /// use pct::Pct;
    ///
    /// assert_eq!(Pct::decode_whatwg("hello%20world"), "hello world");
    /// ```
    pub fn decode_whatwg(input: &str) -> Cow<'_, str> {
        decode(input)
    }

    /// Decode an `application/x-www-form-urlencoded` string.
    ///
    /// `+` is decoded as a space, and valid `%XX` sequences are decoded
    /// to their byte values. Invalid percent-sequences are passed through
    /// as-is (lossy behaviour).
    ///
    /// Equivalent to [`crate::decode_form()`].
    ///
    /// # Examples
    ///
    /// ```
    /// use pct::Pct;
    ///
    /// assert_eq!(Pct::decode_html_form("hello+world"), "hello world");
    /// assert_eq!(Pct::decode_html_form("a%2Bb"), "a+b");
    /// ```
    pub fn decode_html_form(input: &str) -> Cow<'_, str> {
        decode_form(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3986_encode_basic() {
        assert_eq!(Pct::encode_rfc3986("hello world"), "hello%20world");
        assert_eq!(Pct::encode_rfc3986("a+b"), "a%2Bb");
        assert_eq!(Pct::encode_rfc3986("café"), "caf%C3%A9");
        assert_eq!(Pct::encode_rfc3986("a/b?c#d"), "a%2Fb%3Fc%23d");
    }

    #[test]
    fn whatwg_encode_basic() {
        assert_eq!(Pct::encode_whatwg("hello world"), "hello%20world");
        assert_eq!(Pct::encode_whatwg("keep'safe"), "keep'safe");
        assert_eq!(Pct::encode_whatwg("a+b"), "a+b");
        assert_eq!(Pct::encode_whatwg("price=100&sale"), "price=100&sale");
        assert_eq!(Pct::encode_whatwg("path/to/file"), "path/to/file");
        assert_eq!(Pct::encode_whatwg("(parentheses)"), "(parentheses)");
    }

    #[test]
    fn whatwg_encodes_non_url_code_points() {
        assert_eq!(Pct::encode_whatwg("say \"hi\""), "say%20%22hi%22");
        assert_eq!(Pct::encode_whatwg("a<b>c"), "a%3Cb%3Ec");
        assert_eq!(Pct::encode_whatwg("back\\slash"), "back%5Cslash");
        assert_eq!(Pct::encode_whatwg("a^b"), "a%5Eb");
        assert_eq!(Pct::encode_whatwg("a`b"), "a%60b");
        assert_eq!(Pct::encode_whatwg("{a|b}"), "%7Ba%7Cb%7D");
    }

    #[test]
    fn html_form_encode_basic() {
        assert_eq!(Pct::encode_html_form("hello world"), "hello+world");
        assert_eq!(Pct::encode_html_form("a+b"), "a%2Bb");
        assert_eq!(Pct::encode_html_form("key=val&x=1"), "key%3Dval%26x%3D1");
    }

    #[test]
    fn rfc3986_decode_basic() {
        assert_eq!(Pct::decode_rfc3986("hello%20world"), "hello world");
        assert_eq!(Pct::decode_rfc3986("caf%C3%A9"), "café");
    }

    #[test]
    fn whatwg_decode_basic() {
        assert_eq!(Pct::decode_whatwg("hello%20world"), "hello world");
        assert_eq!(Pct::decode_whatwg("caf%C3%A9"), "café");
    }

    #[test]
    fn html_form_decode_basic() {
        assert_eq!(Pct::decode_html_form("hello+world"), "hello world");
        assert_eq!(Pct::decode_html_form("a%2Bb"), "a+b");
    }

    #[test]
    fn roundtrip_rfc3986() {
        let original = "hello world!@# café";
        let encoded = Pct::encode_rfc3986(original);
        let decoded = Pct::decode_rfc3986(&encoded);
        assert_eq!(&*decoded, original);
    }

    #[test]
    fn roundtrip_whatwg() {
        let original = "hello (test) + more! 🎉";
        let encoded = Pct::encode_whatwg(original);
        let decoded = Pct::decode_whatwg(&encoded);
        assert_eq!(&*decoded, original);
    }

    #[test]
    fn roundtrip_html_form() {
        let original = "hello world+test=foo&bar";
        let encoded = Pct::encode_html_form(original);
        let decoded = Pct::decode_html_form(&encoded);
        assert_eq!(&*decoded, original);
    }

    #[test]
    fn rfc3986_vs_whatwg_difference() {
        let input = "keep'safe(yes)!";
        // RFC 3986 encodes ! ' ( )
        let rfc = Pct::encode_rfc3986(input);
        // WHATWG passes them through
        let whatwg = Pct::encode_whatwg(input);
        assert_ne!(rfc, whatwg);
        assert!(rfc.contains("%21")); // ! encoded
        assert!(rfc.contains("%27")); // ' encoded
        assert!(rfc.contains("%28")); // ( encoded
        assert!(rfc.contains("%29")); // ) encoded
        assert_eq!(whatwg, input); // nothing encoded
    }
}
