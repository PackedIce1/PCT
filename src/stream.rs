//! Zero-allocation streaming/iterator API for percent-encoding and decoding.
//!
//! These iterators allow encoding or decoding massive inputs without loading
//! the entire string into RAM. They yield one byte (encoding) or one character
//! (decoding) at a time, requiring **no heap allocation**.
//!
//! This module is always available, even without the `alloc` feature, because
//! the iterators work entirely on borrowed data with stack-allocated internal
//! state.
//!
//! # Encoding
//!
//! [`EncodedBytes`] yields the bytes of the percent-encoded output one at a
//! time. This is useful for writing directly to an I/O sink (file, network,
//! etc.) without allocating an intermediate `String`.
//!
//! # Decoding
//!
//! [`DecodedChars`] yields decoded UTF-8 characters one at a time. It
//! internally buffers decoded bytes and assembles them into valid UTF-8
//! characters, replacing invalid sequences with `U+FFFD` (lossy mode).
//!
//! # `no_std` compatibility
//!
//! Both iterators work in `#![no_std]` environments without `alloc`. They
//! only require `core` and access to the crate's [`EncodeSet`] and hex
//! helpers, which are always available.

use core::iter::FusedIterator;

use crate::hex::{hex_val, is_hex, HEX_UPPER};
use crate::set::EncodeSet;

// ── Encoding iterator ─────────────────────────────────────────────────

/// Internal state machine for [`EncodedBytes`].
enum EncodeState {
    /// Ready to inspect the next input byte.
    Start,
    /// Just yielded `%`, next yield is the high hex nibble.
    YieldHi(u8),
    /// Just yielded the high hex nibble, next yield is the low hex nibble.
    YieldLo(u8),
}

/// A zero-allocation iterator that yields the bytes of a percent-encoded
/// string one at a time.
///
/// Unlike [`crate::encode()`], which returns an owned `String` (or
/// `Cow::Borrowed`), this iterator produces output incrementally with
/// **no heap allocation**. Each call to [`Iterator::next()`] returns one
/// byte of the encoded output.
///
/// For a byte that needs encoding, three consecutive `next()` calls return
/// `%`, then the high hex digit, then the low hex digit.
///
/// # Examples
///
/// ```
/// use pct::{EncodedBytes, EncodeSet};
///
/// let mut iter = EncodedBytes::new("hello world", &EncodeSet::COMPONENT);
/// let bytes: Vec<u8> = iter.collect();
/// assert_eq!(String::from_utf8(bytes).unwrap(), "hello%20world");
/// ```
///
/// Streaming to a writer without intermediate allocation:
///
/// ```ignore
/// use pct::{EncodedBytes, EncodeSet};
/// use std::io::Write;
///
/// let input = "hello world";
/// let mut iter = EncodedBytes::new(input, &EncodeSet::COMPONENT);
/// let mut file = std::fs::File::create("encoded.txt")?;
/// while let Some(byte) = iter.next() {
///     file.write_all(&[byte])?;
/// }
/// ```
pub struct EncodedBytes<'a> {
    input: &'a [u8],
    set: &'a EncodeSet,
    pos: usize,
    state: EncodeState,
}

impl<'a> EncodedBytes<'a> {
    /// Create a new encoding iterator.
    ///
    /// `input` can be `&str` or `&[u8]`. Each byte in `input` that is in
    /// `set` will be percent-encoded as three bytes (`%XX`).
    pub fn new<T: AsRef<[u8]> + ?Sized>(input: &'a T, set: &'a EncodeSet) -> Self {
        Self {
            input: input.as_ref(),
            set,
            pos: 0,
            state: EncodeState::Start,
        }
    }

    /// Create an encoding iterator from raw bytes with a custom encode set.
    pub fn new_bytes(input: &'a [u8], set: &'a EncodeSet) -> Self {
        Self {
            input,
            set,
            pos: 0,
            state: EncodeState::Start,
        }
    }
}

impl Iterator for EncodedBytes<'_> {
    type Item = u8;

    #[inline]
    fn next(&mut self) -> Option<u8> {
        match self.state {
            EncodeState::YieldHi(byte) => {
                self.state = EncodeState::YieldLo(byte);
                Some(HEX_UPPER[(byte >> 4) as usize])
            }
            EncodeState::YieldLo(byte) => {
                self.state = EncodeState::Start;
                Some(HEX_UPPER[(byte & 0x0F) as usize])
            }
            EncodeState::Start => {
                if self.pos >= self.input.len() {
                    return None;
                }
                let byte = self.input[self.pos];
                self.pos += 1;
                if self.set.contains(byte) {
                    self.state = EncodeState::YieldHi(byte);
                    Some(b'%')
                } else {
                    Some(byte)
                }
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.input.len().saturating_sub(self.pos);
        // Each remaining byte produces 1 (passthrough) to 3 (%XX) output bytes.
        (remaining, Some(remaining * 3))
    }
}

impl FusedIterator for EncodedBytes<'_> {}

impl core::fmt::Debug for EncodedBytes<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EncodedBytes")
            .field("remaining", &self.input.len().saturating_sub(self.pos))
            .field("set", self.set)
            .finish()
    }
}

// ── Decoding iterator ─────────────────────────────────────────────────

/// A zero-allocation iterator that yields decoded UTF-8 characters one at
/// a time.
///
/// This iterator percent-decodes the input and assembles the resulting
/// bytes into UTF-8 characters. Invalid `%XX` sequences are replaced with
/// `U+FFFD` (the Unicode replacement character), making this equivalent to
/// [`crate::decode()`] but in streaming form.
///
/// No heap allocation is performed. The iterator maintains a small
/// 4-byte stack buffer for assembling multi-byte UTF-8 characters from
/// decoded `%XX` sequences.
///
/// # Examples
///
/// ```
/// use pct::DecodedChars;
///
/// let mut iter = DecodedChars::new("hello%20world%21");
/// let chars: String = iter.collect();
/// assert_eq!(chars, "hello world!");
/// ```
///
/// Streaming character-by-character:
///
/// ```
/// use pct::DecodedChars;
///
/// let mut iter = DecodedChars::new("caf%C3%A9");
/// assert_eq!(iter.next(), Some('c'));
/// assert_eq!(iter.next(), Some('a'));
/// assert_eq!(iter.next(), Some('f'));
/// assert_eq!(iter.next(), Some('é'));
/// assert_eq!(iter.next(), None);
/// ```
pub struct DecodedChars<'a> {
    input: &'a [u8],
    pos: usize,
    /// Buffer for assembling multi-byte UTF-8 characters from decoded %XX
    /// sequences. At most 4 bytes are needed (maximum UTF-8 code point
    /// length).
    utf8_buf: [u8; 4],
    /// Number of valid bytes currently in `utf8_buf`.
    utf8_len: usize,
}

impl<'a> DecodedChars<'a> {
    /// Create a new decoding iterator (lossy mode).
    ///
    /// Invalid `%XX` sequences are replaced with `U+FFFD`.
    pub fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
            utf8_buf: [0; 4],
            utf8_len: 0,
        }
    }

    /// Create a new decoding iterator from raw bytes.
    ///
    /// This is useful when the input is already available as `&[u8]`.
    /// Invalid `%XX` sequences are replaced with `U+FFFD`.
    pub fn new_bytes(input: &'a [u8]) -> Self {
        Self {
            input,
            pos: 0,
            utf8_buf: [0; 4],
            utf8_len: 0,
        }
    }
}

impl Iterator for DecodedChars<'_> {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        loop {
            // If we have bytes in the UTF-8 buffer, check for a complete char.
            if self.utf8_len > 0 {
                let expected = utf8_seq_len(self.utf8_buf[0]);
                if self.utf8_len >= expected {
                    // Try to decode the UTF-8 sequence.
                    let ch = core::str::from_utf8(&self.utf8_buf[..expected])
                        .map(|s| s.chars().next().unwrap_or('\u{FFFD}'))
                        .unwrap_or('\u{FFFD}');
                    // Shift any remaining bytes to the front of the buffer.
                    let remaining = self.utf8_len - expected;
                    let mut i = 0;
                    while i < remaining {
                        self.utf8_buf[i] = self.utf8_buf[expected + i];
                        i += 1;
                    }
                    self.utf8_len = remaining;
                    return Some(ch);
                }
                // Incomplete sequence — need more bytes from input.
            }

            if self.pos >= self.input.len() {
                // End of input. If we have leftover bytes in the buffer,
                // they represent an incomplete UTF-8 sequence.
                if self.utf8_len > 0 {
                    self.utf8_len = 0;
                    return Some('\u{FFFD}');
                }
                return None;
            }

            let byte = self.input[self.pos];

            if byte == b'%' {
                // Try to decode a %XX sequence.
                if self.pos + 2 < self.input.len()
                    && is_hex(self.input[self.pos + 1])
                    && is_hex(self.input[self.pos + 2])
                {
                    let decoded = (hex_val(self.input[self.pos + 1]) << 4)
                        | hex_val(self.input[self.pos + 2]);
                    self.pos += 3;

                    // Fast path: ASCII byte with empty buffer → yield directly.
                    if self.utf8_len == 0 && decoded < 0x80 {
                        return Some(decoded as char);
                    }

                    // Add to UTF-8 assembly buffer.
                    if self.utf8_len < 4 {
                        self.utf8_buf[self.utf8_len] = decoded;
                        self.utf8_len += 1;
                    } else {
                        // Buffer overflow — shouldn't happen with valid UTF-8.
                        // Flush and yield replacement character.
                        self.utf8_len = 0;
                        return Some('\u{FFFD}');
                    }

                    // Loop back to check if the buffer now has a complete char.
                    continue;
                } else {
                    // Invalid % sequence (truncated or non-hex digits).
                    // Yield replacement character and advance past the %.
                    self.pos += 1;
                    return Some('\u{FFFD}');
                }
            } else {
                // Regular byte — in a percent-encoded string this is always ASCII.
                self.pos += 1;
                return Some(byte as char);
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // Conservative bounds.
        //
        // Each remaining input byte yields at most 1 decoded char — except
        // that a complete `%XX` sequence consumes 3 input bytes while
        // producing at most 1 char (vs. 1 char per passthrough byte). So
        // for the upper bound we subtract 2 per `%XX` sequence found in
        // the remaining input.
        //
        // For the lower bound, the worst case is every 3 bytes forming a
        // `%XX` that yields exactly 1 char, so `remaining / 3`.
        let remaining = self.input.len() - self.pos;

        // Count complete, valid `%XX` sequences in the remaining input.
        // We deliberately mirror the acceptance criteria used by `next()`:
        // `%` followed by two hex digits. Truncated or invalid sequences
        // are treated as passthrough for hint purposes (they yield one
        // `U+FFFD` from the lone `%`, which is still 1 char per byte).
        let mut i = self.pos;
        let mut pct_count = 0;
        while i + 2 < self.input.len() {
            if self.input[i] == b'%' && is_hex(self.input[i + 1]) && is_hex(self.input[i + 2]) {
                pct_count += 1;
                i += 3;
            } else {
                i += 1;
            }
        }

        let min = remaining / 3;
        let max = remaining - 2 * pct_count;
        (min, Some(max))
    }
}
impl FusedIterator for DecodedChars<'_> {}

impl core::fmt::Debug for DecodedChars<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DecodedChars")
            .field("remaining", &self.input.len().saturating_sub(self.pos))
            .field("utf8_buf_len", &self.utf8_len)
            .finish()
    }
}

/// Determine the expected length of a UTF-8 sequence from its leading byte.
///
/// Returns 1 for invalid leading bytes (they'll be handled as replacement
/// characters by the caller).
#[inline]
fn utf8_seq_len(leading_byte: u8) -> usize {
    match leading_byte {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec::Vec;

    // ── EncodedBytes tests ─────────────────────────────────────────

    #[test]
    fn encoded_bytes_simple() {
        let bytes: Vec<u8> = EncodedBytes::new("hello world", &EncodeSet::COMPONENT).collect();
        assert_eq!(String::from_utf8(bytes).unwrap(), "hello%20world");
    }

    #[test]
    fn encoded_bytes_noop() {
        let bytes: Vec<u8> = EncodedBytes::new("hello", &EncodeSet::COMPONENT).collect();
        assert_eq!(String::from_utf8(bytes).unwrap(), "hello");
    }

    #[test]
    fn encoded_bytes_utf8() {
        let bytes: Vec<u8> = EncodedBytes::new("café", &EncodeSet::COMPONENT).collect();
        assert_eq!(String::from_utf8(bytes).unwrap(), "caf%C3%A9");
    }

    #[test]
    fn encoded_bytes_path_set() {
        let bytes: Vec<u8> = EncodedBytes::new("a/b c", &EncodeSet::PATH).collect();
        assert_eq!(String::from_utf8(bytes).unwrap(), "a/b%20c");
    }

    #[test]
    fn encoded_bytes_special_chars() {
        let bytes: Vec<u8> = EncodedBytes::new("a/b?c#d", &EncodeSet::COMPONENT).collect();
        assert_eq!(String::from_utf8(bytes).unwrap(), "a%2Fb%3Fc%23d");
    }

    #[test]
    fn encoded_bytes_binary() {
        let input: &[u8] = &[0x00, 0xFF, 0x20];
        let bytes: Vec<u8> = EncodedBytes::new_bytes(input, &EncodeSet::COMPONENT).collect();
        assert_eq!(bytes, b"%00%FF%20");
    }

    #[test]
    fn encoded_bytes_step_by_step() {
        let mut iter = EncodedBytes::new("a b", &EncodeSet::COMPONENT);
        assert_eq!(iter.next(), Some(b'a'));
        assert_eq!(iter.next(), Some(b'%'));
        assert_eq!(iter.next(), Some(b'2'));
        assert_eq!(iter.next(), Some(b'0'));
        assert_eq!(iter.next(), Some(b'b'));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn encoded_bytes_empty() {
        let bytes: Vec<u8> = EncodedBytes::new("", &EncodeSet::COMPONENT).collect();
        assert!(bytes.is_empty());
    }

    #[test]
    fn encoded_bytes_size_hint() {
        let iter = EncodedBytes::new("hello world", &EncodeSet::COMPONENT);
        let (min, max) = iter.size_hint();
        assert_eq!(min, 11); // input length
        assert_eq!(max, Some(33)); // input length * 3
    }

    #[test]
    fn encoded_bytes_fused() {
        let mut iter = EncodedBytes::new("hello", &EncodeSet::COMPONENT);
        // Drain the iterator
        while iter.next().is_some() {}
        // FusedIterator guarantee: subsequent calls also return None
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }

    // ── DecodedChars tests ─────────────────────────────────────────

    #[test]
    fn decoded_chars_simple() {
        let chars: String = DecodedChars::new("hello%20world").collect();
        assert_eq!(chars, "hello world");
    }

    #[test]
    fn decoded_chars_noop() {
        let chars: String = DecodedChars::new("hello").collect();
        assert_eq!(chars, "hello");
    }

    #[test]
    fn decoded_chars_utf8() {
        let chars: String = DecodedChars::new("caf%C3%A9").collect();
        assert_eq!(chars, "café");
    }

    #[test]
    fn decoded_chars_step_by_step() {
        let mut iter = DecodedChars::new("a%20b");
        assert_eq!(iter.next(), Some('a'));
        assert_eq!(iter.next(), Some(' '));
        assert_eq!(iter.next(), Some('b'));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn decoded_chars_invalid_sequence() {
        let chars: String = DecodedChars::new("test%GG").collect();
        // %GG: G is not hex → U+FFFD, then G and G are regular ASCII.
        assert_eq!(chars, "test\u{FFFD}GG");
    }

    #[test]
    fn decoded_chars_truncated_percent() {
        // "test%2" — % followed by only one hex digit at end.
        let chars: String = DecodedChars::new("test%2").collect();
        assert_eq!(chars, "test\u{FFFD}2");
    }

    #[test]
    fn decoded_chars_bare_percent_at_end() {
        let chars: String = DecodedChars::new("50%").collect();
        // % at end with no following digits → U+FFFD
        assert_eq!(chars, "50\u{FFFD}");
    }

    #[test]
    fn decoded_chars_mixed() {
        let chars: String = DecodedChars::new("a%20b%21c").collect();
        assert_eq!(chars, "a b!c");
    }

    #[test]
    fn decoded_chars_cjk() {
        // 日本語 encoded as UTF-8 percent-encoding
        let chars: String = DecodedChars::new("%E6%97%A5%E6%9C%AC%E8%AA%9E").collect();
        assert_eq!(chars, "日本語");
    }

    #[test]
    fn decoded_chars_emoji() {
        // 🎉 = F0 9F 8E 89
        let chars: String = DecodedChars::new("%F0%9F%8E%89").collect();
        assert_eq!(chars, "🎉");
    }

    #[test]
    fn decoded_chars_empty() {
        let chars: String = DecodedChars::new("").collect();
        assert!(chars.is_empty());
    }

    #[test]
    fn decoded_chars_incomplete_utf8_at_end() {
        // %C3 without a continuation byte at end of input.
        let chars: String = DecodedChars::new("x%C3").collect();
        // x, then %C3 decodes to 0xC3 (incomplete 2-byte UTF-8), then EOF.
        // The incomplete sequence should yield U+FFFD.
        assert_eq!(chars, "x\u{FFFD}");
    }

    #[test]
    fn decoded_chars_fused() {
        let mut iter = DecodedChars::new("hello");
        while iter.next().is_some() {}
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn decoded_chars_size_hint() {
        let iter = DecodedChars::new("hello%20world");
        let (min, max) = iter.size_hint();
        assert_eq!(min, 4); // 11 / 3 = 3 (floor), but there's 1 %XX = 1 char from 3 bytes
        assert_eq!(max, Some(11));
    }

    // ── Round-trip test ───────────────────────────────────────────

    #[test]
    fn roundtrip_via_iterators() {
        let original = "hello world! café 日本語";
        let set = EncodeSet::COMPONENT;

        // Encode via iterator
        let encoded_bytes: Vec<u8> = EncodedBytes::new(original, &set).collect();
        let encoded_str = String::from_utf8(encoded_bytes).unwrap();

        // Decode via iterator
        let decoded: String = DecodedChars::new(&encoded_str).collect();
        assert_eq!(decoded, original);
    }
}
