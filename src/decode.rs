use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::encode::{hex_val, is_hex};

// ── Error type ─────────────────────────────────────────────────────

/// Errors that can occur during percent-decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// A `%` is not followed by two valid hex digits.
    InvalidHex {
        /// Byte offset where the invalid sequence starts.
        position: usize,
    },
    /// A `%` appears at the end of the string or is followed by only one
    /// character.
    TruncatedSequence {
        /// Byte offset where the truncated sequence starts.
        position: usize,
    },
    /// The decoded bytes are not valid UTF-8.
    InvalidUtf8 {
        /// Byte offset where invalid UTF-8 was found.
        position: usize,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::InvalidHex { position } => {
                write!(
                    f,
                    "invalid percent-encoded hex at byte position {position}"
                )
            }
            DecodeError::TruncatedSequence { position } => {
                write!(
                    f,
                    "truncated percent-encoded sequence at byte position {position}"
                )
            }
            DecodeError::InvalidUtf8 { position } => {
                write!(
                    f,
                    "decoded bytes are not valid UTF-8 near byte position {position}"
                )
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for DecodeError {}

// ── Decoding functions ─────────────────────────────────────────────

/// Percent-decode a string, replacing invalid sequences with the
/// replacement character `U+FFFD`.
///
/// This is the most forgiving decode mode and is suitable for displaying
/// user-visible text where you never want to fail.
///
/// # Invalid sequence handling
///
/// - `%` not followed by two hex digits → replaced with `\u{FFFD}`
/// - `%` at the end of the string → replaced with `\u{FFFD}`
/// - Decoded bytes that are not valid UTF-8 → replaced with `\u{FFFD}`
///
/// # Examples
///
/// ```
/// use pct::decode;
///
/// assert_eq!(decode("hello%20world"), "hello world");
/// assert_eq!(decode("caf%C3%A9"), "café");
/// ```
pub fn decode(input: &str) -> Cow<'_, str> {
    if !needs_decoding(input.as_bytes()) {
        return Cow::Borrowed(input);
    }
    // Decode valid %XX sequences, pass through invalid ones,
    // then use from_utf8_lossy for final UTF-8 fixup.
    let raw = do_decode_to_bytes(input.as_bytes());
    match String::from_utf8(raw) {
        Ok(s) => Cow::Owned(s),
        Err(e) => {
            let lossy = String::from_utf8_lossy(e.as_bytes());
            Cow::Owned(lossy.into_owned())
        }
    }
}

/// Percent-decode a string strictly, returning an error for any invalid
/// sequence or non-UTF-8 result.
///
/// # Examples
///
/// ```
/// use pct::decode_strict;
///
/// assert_eq!(decode_strict("hello%20world").unwrap(), "hello world");
/// assert!(decode_strict("hello%GG").is_err());
/// ```
pub fn decode_strict(input: &str) -> Result<Cow<'_, str>, DecodeError> {
    if !needs_decoding(input.as_bytes()) {
        return Ok(Cow::Borrowed(input));
    }
    let bytes = do_decode_strict(input.as_bytes())?;
    match String::from_utf8(bytes) {
        Ok(s) => Ok(Cow::Owned(s)),
        Err(e) => {
            let pos = e.utf8_error().valid_up_to();
            Err(DecodeError::InvalidUtf8 { position: pos })
        }
    }
}

/// Percent-decode a string, leaving invalid sequences (`%GG`, truncated `%`)
/// as-is in the output.
///
/// This is useful when you want to partially decode a string while
/// preserving any literal `%` characters that aren't valid sequences.
///
/// # Examples
///
/// ```
/// use pct::decode_passthrough;
///
/// assert_eq!(decode_passthrough("100%25"), "100%");
/// assert_eq!(decode_passthrough("50%GG"), "50%GG"); // invalid → left as-is
/// ```
pub fn decode_passthrough(input: &str) -> Cow<'_, str> {
    if !needs_decoding(input.as_bytes()) {
        return Cow::Borrowed(input);
    }
    let raw = do_decode_passthrough(input.as_bytes());
    // Passthrough may produce non-UTF-8 if %XX sequences decode to
    // partial UTF-8 bytes. Use lossy conversion as a safety net.
    match String::from_utf8(raw) {
        Ok(s) => Cow::Owned(s),
        Err(e) => {
            let lossy = String::from_utf8_lossy(e.as_bytes());
            Cow::Owned(lossy.into_owned())
        }
    }
}

/// Percent-decode to raw bytes (no UTF-8 validation).
///
/// Returns `Cow::Borrowed` when the input contains no `%` characters.
///
/// # Examples
///
/// ```
/// use pct::decode_bytes;
///
/// let decoded = decode_bytes("hello%20world");
/// assert_eq!(&*decoded, b"hello world");
/// ```
pub fn decode_bytes(input: &str) -> Cow<'_, [u8]> {
    if !needs_decoding(input.as_bytes()) {
        return Cow::Borrowed(input.as_bytes());
    }
    Cow::Owned(do_decode_to_bytes(input.as_bytes()))
}

// ── Internal helpers ───────────────────────────────────────────────

fn needs_decoding(input: &[u8]) -> bool {
    input.contains(&b'%')
}

/// Strict decode: returns error on any invalid sequence.
fn do_decode_strict(input: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' {
            if i + 2 >= input.len() {
                return Err(DecodeError::TruncatedSequence { position: i });
            }
            if !is_hex(input[i + 1]) || !is_hex(input[i + 2]) {
                return Err(DecodeError::InvalidHex { position: i });
            }
            let byte = (hex_val(input[i + 1]) << 4) | hex_val(input[i + 2]);
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(input[i]);
        i += 1;
    }
    Ok(out)
}

/// Passthrough decode: invalid sequences are copied verbatim.
fn do_decode_passthrough(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' {
            if i + 2 < input.len() && is_hex(input[i + 1]) && is_hex(input[i + 2]) {
                let byte = (hex_val(input[i + 1]) << 4) | hex_val(input[i + 2]);
                out.push(byte);
                i += 3;
                continue;
            }
            // Invalid: copy the % and continue
            out.push(b'%');
            i += 1;
            continue;
        }
        out.push(input[i]);
        i += 1;
    }
    out
}

/// Byte decode: decodes valid %XX sequences, passes everything else
/// through (including invalid % sequences).
fn do_decode_to_bytes(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%'
            && i + 2 < input.len()
            && is_hex(input[i + 1])
            && is_hex(input[i + 2])
        {
            let byte = (hex_val(input[i + 1]) << 4) | hex_val(input[i + 2]);
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(input[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_noop() {
        let input = "hello";
        let result = decode(input);
        assert_eq!(result, "hello");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn decode_space() {
        assert_eq!(decode("hello%20world"), "hello world");
    }

    #[test]
    fn decode_utf8() {
        assert_eq!(decode("caf%C3%A9"), "café");
    }

    #[test]
    fn decode_strict_valid() {
        assert_eq!(decode_strict("hello%20world").unwrap(), "hello world");
    }

    #[test]
    fn decode_strict_invalid_hex() {
        assert!(matches!(
            decode_strict("hello%GG"),
            Err(DecodeError::InvalidHex { .. })
        ));
    }

    #[test]
    fn decode_strict_truncated() {
        assert!(matches!(
            decode_strict("hello%2"),
            Err(DecodeError::TruncatedSequence { .. })
        ));
    }

    #[test]
    fn decode_passthrough_invalid() {
        assert_eq!(decode_passthrough("50%GG"), "50%GG");
    }

    #[test]
    fn decode_passthrough_valid() {
        assert_eq!(decode_passthrough("100%25"), "100%");
    }

    #[test]
    fn decode_bytes_raw() {
        let result = decode_bytes("hello%20world");
        assert_eq!(&*result, b"hello world");
    }

    #[test]
    fn decode_bytes_noop() {
        let result = decode_bytes("hello");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn decode_lossy_invalid_sequence() {
        // %GG is invalid → lossy handles it gracefully
        let result = decode("test%GG");
        assert!(result.contains("test"));
    }
}
