use core::fmt;

/// A set of bytes that should be percent-encoded.
///
/// Uses a compact bitmask (`[u64; 4]` = 256 bits, one per byte value) for
/// efficient lookup. Build sets with the [`add`](Self::add) /
/// [`remove`](Self::remove) builder pattern, or use one of the predefined
/// constants.
///
/// `EncodeSet` is always available — it does **not** require the `alloc`
/// feature, making it usable in `#![no_std]` environments without a heap.
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

    /// Returns the raw 256-bit bitmask as four `u64` words.
    ///
    /// Word 0 covers bytes `0x00–0x3F`, word 1 covers `0x40–0x7F`,
    /// word 2 covers `0x80–0xBF`, word 3 covers `0xC0–0xFF`.
    ///
    /// Useful for callers that want to do their own SIMD-style batch
    /// checks against the bitmask.
    #[inline]
    pub const fn bits(&self) -> &[u64; 4] {
        &self.bits
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

    /// Everything that is **not** a [WHATWG URL Standard "URL code point"].
    ///
    /// URL code points are: `A-Z a-z 0-9 ! $ & ' ( ) * + , - . / : ; = ? @ _ ~`.
    ///
    /// This set is **more permissive** than [`COMPONENT`](Self::COMPONENT) —
    /// characters like `!`, `'`, `(`, `)`, `*`, `+`, `,`, `;`, `=`, `?`,
    /// `@`, and `/` pass through unencoded. Only C0 controls, space,
    /// `"`, `#`, `<`, `>`, `\`, `^`, `` ` ``, `{`, `|`, `}`, DEL, and
    /// non-ASCII bytes are encoded.
    ///
    /// Use this when you want WHATWG-compatible encoding that preserves
    /// common URL punctuation. This matches the behaviour of browsers'
    /// `URL` constructor when setting properties like `pathname`.
    ///
    /// [WHATWG URL Standard "URL code point"]: https://url.spec.whatwg.org/#url-code-points
    pub const WHATWG: Self = Self::new()
        // C0 controls (0x00–0x1F) and space (0x20)
        .add_range(0x00, 0x20)
        // " (0x22) — not a URL code point
        .add(0x22)
        // # (0x23) — not a URL code point (handled as delimiter by the parser)
        .add(0x23)
        // < (0x3C)
        .add(0x3C)
        // > (0x3E)
        .add(0x3E)
        // \ (0x5C)
        .add(0x5C)
        // ^ (0x5E)
        .add(0x5E)
        // ` (0x60)
        .add(0x60)
        // { (0x7B) | (0x7C) } (0x7D)
        .add(0x7B)
        .add(0x7C)
        .add(0x7D)
        // DEL (0x7F)
        .add(0x7F)
        // Non-ASCII (0x80–0xFF)
        .add_range(0x80, 0xFF);
}

impl fmt::Debug for EncodeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Build the debug list directly against the formatter — no Vec
        // allocation needed. This works in `#![no_std]` without `alloc`.
        let mut list = f.debug_list();
        for b in 0u16..=255 {
            if self.contains(b as u8) {
                list.entry(&(b as u8));
            }
        }
        list.finish()
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

    #[cfg(feature = "alloc")]
    #[test]
    fn debug_does_not_allocate() {
        // The Debug impl writes directly to the formatter (no Vec), so it
        // works without alloc. We just exercise it here — the test
        // itself uses format! (which requires alloc) but the impl under
        // test does not.
        //
        // The output is a debug_list of bytes, e.g. `[0, 1, 2, ..., 255]`.
        let set = EncodeSet::COMPONENT;
        let s = alloc::format!("{set:?}");
        assert!(s.starts_with('['), "debug_list should start with '[': {s}");
        assert!(s.contains("32"), "COMPONENT contains space (0x20=32): {s}");

        let empty = EncodeSet::new();
        let s2 = alloc::format!("{empty:?}");
        assert_eq!(s2, "[]", "empty set should format as []: {s2}");
    }

    #[test]
    fn bits_accessor() {
        let set = EncodeSet::new().add(b' ');
        let bits = set.bits();
        // Space is byte 0x20 = 32, which is bit 32 in word 0.
        assert_eq!(bits[0] & (1u64 << 32), 1u64 << 32);
    }

    // ── WHATWG set tests ────────────────────────────────────────

    #[test]
    fn whatwg_allows_url_code_points() {
        let set = EncodeSet::WHATWG;
        // URL code points should NOT be in the WHATWG encode set
        for b in b'A'..=b'Z' {
            assert!(!set.contains(b), "WHATWG should not encode '{b}'");
        }
        for b in b'a'..=b'z' {
            assert!(!set.contains(b), "WHATWG should not encode '{b}'");
        }
        for b in b'0'..=b'9' {
            assert!(!set.contains(b), "WHATWG should not encode '{b}'");
        }
        // URL code point punctuation
        assert!(!set.contains(b'!'));
        assert!(!set.contains(b'$'));
        assert!(!set.contains(b'&'));
        assert!(!set.contains(b'\''));
        assert!(!set.contains(b'('));
        assert!(!set.contains(b')'));
        assert!(!set.contains(b'*'));
        assert!(!set.contains(b'+'));
        assert!(!set.contains(b','));
        assert!(!set.contains(b'-'));
        assert!(!set.contains(b'.'));
        assert!(!set.contains(b'/'));
        assert!(!set.contains(b':'));
        assert!(!set.contains(b';'));
        assert!(!set.contains(b'='));
        assert!(!set.contains(b'?'));
        assert!(!set.contains(b'@'));
        assert!(!set.contains(b'_'));
        assert!(!set.contains(b'~'));
    }

    #[test]
    fn whatwg_encodes_non_url_code_points() {
        let set = EncodeSet::WHATWG;
        // Non-URL-code-point characters should be encoded
        assert!(set.contains(b' '));  // space
        assert!(set.contains(b'"'));  // "
        assert!(set.contains(b'#'));  // #
        assert!(set.contains(b'<'));  // <
        assert!(set.contains(b'>'));  // >
        assert!(set.contains(b'\\')); // backslash
        assert!(set.contains(b'^'));  // ^
        assert!(set.contains(b'`'));  // backtick
        assert!(set.contains(b'{'));  // {
        assert!(set.contains(b'|'));  // |
        assert!(set.contains(b'}'));  // }
    }

    #[test]
    fn whatwg_vs_component_difference() {
        let comp = EncodeSet::COMPONENT;
        let whatwg = EncodeSet::WHATWG;
        // These chars are in COMPONENT (encoded) but NOT in WHATWG (passed through)
        assert!(comp.contains(b'!'));
        assert!(!whatwg.contains(b'!'));
        assert!(comp.contains(b'('));
        assert!(!whatwg.contains(b'('));
        assert!(comp.contains(b'+' ));
        assert!(!whatwg.contains(b'+'));
        assert!(comp.contains(b','));
        assert!(!whatwg.contains(b','));
        assert!(comp.contains(b'/'));
        assert!(!whatwg.contains(b'/'));
        assert!(comp.contains(b':'));
        assert!(!whatwg.contains(b':'));
        assert!(comp.contains(b';'));
        assert!(!whatwg.contains(b';'));
        assert!(comp.contains(b'='));
        assert!(!whatwg.contains(b'='));
        assert!(comp.contains(b'?'));
        assert!(!whatwg.contains(b'?'));
        assert!(comp.contains(b'@'));
        assert!(!whatwg.contains(b'@'));
    }
}
