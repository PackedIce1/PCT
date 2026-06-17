//! Compile-time (const) percent-encoding helpers.
//!
//! These functions are always available (no `alloc` required) and can be
//! used in `const` contexts to pre-compute percent-encoded strings at
//! compile time.
//!
//! For the convenient macro, see [`crate::const_encode!`].

use crate::hex::HEX_UPPER;
use crate::set::EncodeSet;

/// Maximum input length (in bytes) supported by
/// [`const_encode!`](crate::const_encode).
///
/// Inputs longer than this will cause a compile-time error from the macro.
/// The limit exists because the macro uses a fixed-size internal buffer.
pub const MAX_CONST_INPUT_LEN: usize = 1024;

/// Buffer size used internally by [`const_encode!`](crate::const_encode).
///
/// Equals `MAX_CONST_INPUT_LEN * 3` (worst case: every byte encoded as
/// `%XX`).
pub const CONST_ENCODE_BUF_SIZE: usize = MAX_CONST_INPUT_LEN * 3;

/// Percent-encode `input` bytes into `buf`, returning the number of bytes
/// written.
///
/// This is a `const fn` and can be used in const contexts. The caller must
/// ensure `buf` is large enough (at least [`const_encoded_len`](crate::const_encoded_len)
/// bytes).
///
/// # Examples
///
/// ```
/// use pct::{const_encode_to_buf, const_encoded_len, EncodeSet};
///
/// const INPUT: &[u8] = b"hello world";
/// let mut buf = [0u8; 64];
/// let len = const_encode_to_buf(INPUT, &EncodeSet::COMPONENT, &mut buf);
/// assert_eq!(&buf[..len], b"hello%20world");
/// assert_eq!(len, const_encoded_len(INPUT, &EncodeSet::COMPONENT));
/// ```
pub const fn const_encode_to_buf(input: &[u8], set: &EncodeSet, buf: &mut [u8]) -> usize {
    let mut i = 0;
    let mut o = 0;
    while i < input.len() {
        let b = input[i];
        if set.contains(b) {
            buf[o] = b'%';
            buf[o + 1] = HEX_UPPER[(b >> 4) as usize];
            buf[o + 2] = HEX_UPPER[(b & 0x0F) as usize];
            o += 3;
        } else {
            buf[o] = b;
            o += 1;
        }
        i += 1;
    }
    o
}

/// Compute the length of the percent-encoded output without actually
/// encoding.
///
/// This is a `const fn` and can be used to pre-size buffers in const
/// contexts.
///
/// # Examples
///
/// ```
/// use pct::{const_encoded_len, EncodeSet};
///
/// const INPUT: &[u8] = b"hello world";
/// assert_eq!(const_encoded_len(INPUT, &EncodeSet::COMPONENT), 13); // 5 + 3 + 5
/// ```
pub const fn const_encoded_len(input: &[u8], set: &EncodeSet) -> usize {
    let mut len = 0;
    let mut i = 0;
    while i < input.len() {
        if set.contains(input[i]) {
            len += 3;
        } else {
            len += 1;
        }
        i += 1;
    }
    len
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::set::EncodeSet;

    #[test]
    fn const_encode_basic() {
        let mut buf = [0u8; 64];
        let len = const_encode_to_buf(b"hello world", &EncodeSet::COMPONENT, &mut buf);
        assert_eq!(&buf[..len], b"hello%20world");
    }

    #[test]
    fn const_encode_noop() {
        let mut buf = [0u8; 64];
        let len = const_encode_to_buf(b"hello", &EncodeSet::COMPONENT, &mut buf);
        assert_eq!(&buf[..len], b"hello");
        assert_eq!(len, 5);
    }

    #[test]
    fn const_encode_special_chars() {
        let mut buf = [0u8; 128];
        let len = const_encode_to_buf(b"a/b?c#d", &EncodeSet::COMPONENT, &mut buf);
        assert_eq!(&buf[..len], b"a%2Fb%3Fc%23d");
    }

    #[test]
    fn const_encode_utf8() {
        let mut buf = [0u8; 64];
        let len = const_encode_to_buf("café".as_bytes(), &EncodeSet::COMPONENT, &mut buf);
        assert_eq!(&buf[..len], b"caf%C3%A9");
    }

    #[test]
    fn const_encode_binary() {
        let mut buf = [0u8; 64];
        let input: &[u8] = &[0x00, 0xFF, 0x20];
        let len = const_encode_to_buf(input, &EncodeSet::COMPONENT, &mut buf);
        assert_eq!(&buf[..len], b"%00%FF%20");
    }

    #[test]
    fn const_encoded_len_matches() {
        let cases: &[&[u8]] = &[
            b"hello",
            b"hello world",
            b"a/b?c#d",
            "café".as_bytes(),
            &[0x00, 0xFF, 0x20],
        ];
        let set = EncodeSet::COMPONENT;
        for &input in cases {
            let mut buf = [0u8; 256];
            let len = const_encode_to_buf(input, &set, &mut buf);
            assert_eq!(
                len,
                const_encoded_len(input, &set),
                "length mismatch for input: {input:?}"
            );
        }
    }

    #[test]
    fn const_encoded_len_path_set() {
        let input = b"a/b c";
        let set = EncodeSet::PATH;
        assert_eq!(const_encoded_len(input, &set), 7); // a/b%20c = 7
    }

    #[test]
    fn const_encode_whatwg_set() {
        let mut buf = [0u8; 128];
        let set = EncodeSet::WHATWG;
        // ! ' ( ) * + are URL code points, should NOT be encoded
        let len = const_encode_to_buf(b"keep'safe(yes)!", &set, &mut buf);
        assert_eq!(&buf[..len], b"keep'safe(yes)!");
        // Space and " should be encoded
        let len2 = const_encode_to_buf(b"say \"hello\"", &set, &mut buf);
        assert_eq!(&buf[..len2], b"say%20%22hello%22");
    }
}
