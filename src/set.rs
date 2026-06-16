use alloc::vec::Vec;
use core::fmt;

/// A set of bytes that should be percent-encoded.
///
/// Uses a compact bitmask (`[u64; 4]` = 256 bits, one per byte value) for
/// efficient lookup. Build sets with the [`add`](Self::add) /
/// [`remove`](Self::remove) builder pattern, or use one of the predefined
/// constants.
///
/// # Examples
///
/// ```
/// use pct::EncodeSet;
///
/// // Custom set: encode spaces and angle brackets
/// const MY_SET: &EncodeSet = &EncodeSet::new().add(b' ').add(b'<').add(b'>');
///
/// assert!(MY_SET.contains(b' '));
/// assert!(!MY_SET.contains(b'a'));
/// ```
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EncodeSet {
    bits: [u64; 4],
}

impl Default for EncodeSet {
    fn default() -> Self {
        Self::new()
    }
}

impl EncodeSet {
    // ------------------------------------------------------------------
    // Construction
    // ------------------------------------------------------------------

    /// Create an empty set — no bytes will be encoded.
    pub const fn new() -> Self {
        EncodeSet { bits: [0; 4] }
    }

    /// Add a byte to the set.
    pub const fn add(self, byte: u8) -> Self {
        let idx = byte as usize / 64;
        let bit = byte as usize % 64;
        let mut bits = self.bits;
        bits[idx] |= 1u64 << bit;
        EncodeSet { bits }
    }

    /// Remove a byte from the set.
    pub const fn remove(self, byte: u8) -> Self {
        let idx = byte as usize / 64;
        let bit = byte as usize % 64;
        let mut bits = self.bits;
        bits[idx] &= !(1u64 << bit);
        EncodeSet { bits }
    }

    /// Add an inclusive range of bytes to the set.
    pub const fn add_range(self, start: u8, end: u8) -> Self {
        let mut set = self;
        let mut i = start as u32;
        while i <= end as u32 {
            set = set.add(i as u8);
            i += 1;
        }
        set
    }

    /// Remove an inclusive range of bytes from the set.
    pub const fn remove_range(self, start: u8, end: u8) -> Self {
        let mut set = self;
        let mut i = start as u32;
        while i <= end as u32 {
            set = set.remove(i as u8);
            i += 1;
        }
        set
    }

    // ------------------------------------------------------------------
    // Query
    // ------------------------------------------------------------------

    /// Returns `true` if the byte is in the set (should be encoded).
    #[inline]
    pub const fn contains(&self, byte: u8) -> bool {
        let idx = byte as usize / 64;
        let bit = byte as usize % 64;
        (self.bits[idx] >> bit) & 1 != 0
    }

    // ------------------------------------------------------------------
    // Predefined sets
    // ------------------------------------------------------------------

    /// C0 controls (0x00–0x1F) and DEL (0x7F).
    ///
    /// Minimal set — only encodes control characters.
    pub const CONTROLS: Self = Self::new().add_range(0x00, 0x1F).add(0x7F);

    /// Everything that is not an ASCII letter or digit.
    ///
    /// This is more aggressive than [`COMPONENT`](Self::COMPONENT) because it
    /// also encodes unreserved punctuation (`-`, `.`, `_`, `~`).
    pub const NON_ALPHANUMERIC: Self = Self::new()
        .add_range(0x00, 0x2F)
        .add_range(0x3A, 0x40)
        .add_range(0x5B, 0x60)
        .add_range(0x7B, 0xFF);

    /// Encode everything that is not an RFC 3986 *unreserved* character.
    ///
    /// Unreserved = `A–Z a–z 0–9 - . _ ~`
    ///
    /// This is the safest default for encoding an individual URL component
    /// value and is what [`crate::encode()`] uses.
    pub const COMPONENT: Self = Self::new()
        .add_range(0x00, 0x2C) // before '-'
        .add(0x2F) // '/'
        .add_range(0x3A, 0x40) // : ; < = > ? @
        .add_range(0x5B, 0x5E) // [ \ ] ^
        .add(0x60) // `
        .add_range(0x7B, 0x7D) // { | }
        .add(0x7F) // DEL
        .add_range(0x80, 0xFF); // high bytes

    /// Like [`COMPONENT`](Self::COMPONENT), but keeps `/` unencoded.
    ///
    /// Use this when encoding a **full path** string (e.g. `a/b/c`) where
    /// the `/` separator must be preserved.
    pub const PATH: Self = Self::COMPONENT.remove(b'/');

    /// Like [`COMPONENT`](Self::COMPONENT), but keeps `?`, `=`, `&`
    /// unencoded.
    ///
    /// Use this when encoding an **entire query string**
    /// (e.g. `key=val&flag=yes`), not individual parameter values.
    /// For a single parameter value, use [`COMPONENT`](Self::COMPONENT)
    /// instead.
    pub const QUERY: Self = Self::COMPONENT.remove(b'?').remove(b'=').remove(b'&');

    /// Like [`COMPONENT`](Self::COMPONENT), but keeps `#` unencoded.
    ///
    /// Use this when encoding a **full fragment** string where the `#`
    /// delimiter has already been stripped by a URL parser.
    pub const FRAGMENT: Self = Self::COMPONENT.remove(b'#');
}

impl fmt::Debug for EncodeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut bytes = Vec::new();
        for b in 0u16..=255 {
            if self.contains(b as u8) {
                bytes.push(b as u8);
            }
        }
        f.debug_struct("EncodeSet")
            .field("encoded_bytes", &bytes)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::EncodeSet;

    #[test]
    fn empty_set_contains_nothing() {
        let set = EncodeSet::new();
        for b in 0u8..=255 {
            assert!(!set.contains(b), "empty set should not contain {b}");
        }
    }

    #[test]
    fn add_and_remove() {
        let set = EncodeSet::new().add(b'a').add(b'b');
        assert!(set.contains(b'a'));
        assert!(set.contains(b'b'));
        assert!(!set.contains(b'c'));

        let set = set.remove(b'a');
        assert!(!set.contains(b'a'));
        assert!(set.contains(b'b'));
    }

    #[test]
    fn add_range() {
        let set = EncodeSet::new().add_range(b'0', b'9');
        for b in b'0'..=b'9' {
            assert!(set.contains(b), "should contain '{b}'");
        }
        assert!(!set.contains(b'/'));
        assert!(!set.contains(b':'));
    }

    #[test]
    fn component_set_unreserved_not_encoded() {
        let set = EncodeSet::COMPONENT;
        // Unreserved characters
        for b in b'A'..=b'Z' {
            assert!(!set.contains(b), "A-Z should not be in COMPONENT: {b}");
        }
        for b in b'a'..=b'z' {
            assert!(!set.contains(b), "a-z should not be in COMPONENT: {b}");
        }
        for b in b'0'..=b'9' {
            assert!(!set.contains(b), "0-9 should not be in COMPONENT: {b}");
        }
        assert!(!set.contains(b'-'));
        assert!(!set.contains(b'.'));
        assert!(!set.contains(b'_'));
        assert!(!set.contains(b'~'));
    }

    #[test]
    fn component_set_reserved_encoded() {
        let set = EncodeSet::COMPONENT;
        assert!(set.contains(b' '));
        assert!(set.contains(b'%'));
        assert!(set.contains(b'/'));
        assert!(set.contains(b'?'));
        assert!(set.contains(b'#'));
        assert!(set.contains(b'['));
        assert!(set.contains(b']'));
        assert!(set.contains(b'@'));
        assert!(set.contains(b'!'));
        assert!(set.contains(b'$'));
        assert!(set.contains(b'&'));
        assert!(set.contains(b'\''));
        assert!(set.contains(b'('));
        assert!(set.contains(b')'));
        assert!(set.contains(b'*'));
        assert!(set.contains(b'+'));
        assert!(set.contains(b','));
        assert!(set.contains(b';'));
        assert!(set.contains(b'='));
    }

    #[test]
    fn path_set_keeps_slash() {
        assert!(!EncodeSet::PATH.contains(b'/'));
        assert!(EncodeSet::COMPONENT.contains(b'/'));
    }

    #[test]
    fn query_set_keeps_delimiters() {
        let set = EncodeSet::QUERY;
        assert!(!set.contains(b'?'));
        assert!(!set.contains(b'='));
        assert!(!set.contains(b'&'));
    }

    #[test]
    fn fragment_set_keeps_hash() {
        assert!(!EncodeSet::FRAGMENT.contains(b'#'));
    }
}
