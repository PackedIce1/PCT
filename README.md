# pct

Percent-encoding and decoding for URLs — pure Rust, zero dependencies, `no_std` + `alloc`.

A modern alternative to the `percent-encoding` crate that fixes its long-standing pain points while keeping a clean, ergonomic API.

## Why `pct`?

The `percent-encoding` crate is widely used but has several well-known issues that have remained open for years. `pct` fixes all of them:

| Issue | `percent-encoding` | `pct` |
|-------|--------------------|-------|
| [#503](https://github.com/servo/rust-url/issues/503) — bare `%` not encoded | `%` is left as-is → broken round-trips | `encode("100%")` → `"100%25"` |
| [#416](https://github.com/servo/rust-url/issues/416) / [#482](https://github.com/servo/rust-url/issues/482) — `+` ambiguity | `+` is not encoded; no form-urlencoded support | `encode("a+b")` → `"a%2Bb"`; dedicated `encode_form`/`decode_form` |
| Few predefined sets | Only `CONTROLS` and `NON_ALPHANUMERIC` | `COMPONENT`, `PATH`, `QUERY`, `FRAGMENT`, `CONTROLS`, `NON_ALPHANUMERIC` |
| No form-urlencoded support | Requires a separate crate | Built-in `encode_form` / `decode_form` |
| No binary data support | Requires low-level API | `encode_bytes(&[u8])` works directly |
| Idempotent encoding | Double-encoding is easy | `encode("foo%20bar")` is a no-op |

## Quick start

```rust
use pct::{encode, decode, encode_form, decode_form};

// URL percent-encoding (idempotent — already-encoded sequences are preserved)
assert_eq!(encode("hello world"), "hello%20world");
assert_eq!(encode("100%"), "100%25");            // bare % → encoded
assert_eq!(encode("foo%20bar"), "foo%20bar");     // already encoded → no-op

// URL percent-decoding
assert_eq!(decode("hello%20world"), "hello world");

// Form-urlencoded (space → +, + → %2B)
assert_eq!(encode_form("hello world"), "hello+world");
assert_eq!(decode_form("hello+world"), "hello world");
assert_eq!(encode_form("a+b"), "a%2Bb");          // literal + → %2B
```

## Features

- **Pure Rust** — no C dependencies, no build scripts
- **Zero dependencies** — nothing in your lockfile
- **`no_std` + `alloc`** — works in embedded and WASM targets
- **Idempotent encoding** — calling `encode()` twice always produces the same result
- **Hex normalization** — lowercase `%2f` is normalised to `%2F`
- **Predefined context sets** — `COMPONENT`, `PATH`, `QUERY`, `FRAGMENT`
- **Multiple decode strategies** — lossy, strict, passthrough, and raw bytes
- **Built-in `application/x-www-form-urlencoded`** — no extra crate needed
- **Arbitrary binary data** — `encode_bytes()` works on `&[u8]`
- **Normalization** — canonical form with uppercase hex and decoded unreserved chars
- **Validation** — quick `is_valid()` check for well-formed encoding
- **`Display` wrapper** — `Encoded("foo/bar")` for inline formatting
- **Extension trait** — `"hello world".percent_encode()` on `&str`
- **IRI → URI** — optional `iri` feature for internationalized identifiers

## API overview

### Encoding

```rust
use pct::{encode, encode_with, encode_raw, encode_bytes, EncodeSet};

// Simple — uses COMPONENT set (everything except RFC 3986 unreserved chars)
let s = encode("hello world");

// Context-specific sets
let s = encode_with("a/b c", &EncodeSet::PATH);      // keeps /
let s = encode_with("k=v&x=1 y", &EncodeSet::QUERY);  // keeps ? = &

// Raw mode (encodes % too — use when input is known to be unencoded)
let s = encode_raw("foo%20bar", &EncodeSet::COMPONENT); // → "foo%2520bar"

// Binary data
let s = encode_bytes(&[0x00, 0xFF, 0x20], &EncodeSet::COMPONENT);
```

### Convenience functions

```rust
use pct::{encode_for_path, encode_for_query, encode_for_fragment, encode_for_component};

encode_for_path("a/b c");       // "a/b%20c"
encode_for_query("k=v&x=1 y");  // "k=v&x=1%20y"
encode_for_fragment("a#b c");   // "a#b%20c"
encode_for_component("a/b?c");  // "a%2Fb%3Fc"
```

### Decoding

```rust
use pct::{decode, decode_strict, decode_passthrough, decode_bytes, DecodeError};

// Lossy (default) — invalid sequences → U+FFFD
let s = decode("hello%20world"); // "hello world"

// Strict — errors on invalid input
match decode_strict("hello%GG") {
    Err(DecodeError::InvalidHex { position }) => { /* handle */ }
    _ => {}
}

// Passthrough — leaves invalid sequences as-is
let s = decode_passthrough("50%GG"); // "50%GG"

// Raw bytes (no UTF-8 validation)
let bytes = decode_bytes("hello%20world"); // b"hello world"
```

### Form-urlencoded

```rust
use pct::{encode_form, decode_form, encode_form_bytes};

// Space → +, literal + → %2B
encode_form("hello world");    // "hello+world"
encode_form("a+b");            // "a%2Bb"
decode_form("hello+world");    // "hello world"
decode_form("a%2Bb");          // "a+b"

// Binary data
encode_form_bytes(b"\xC3\xA9"); // "%C3%A9"
```

### Normalization & validation

```rust
use pct::{normalize, is_valid};

normalize("%2f%2F");     // "%2F%2F" (uppercase hex)
normalize("%7E");        // "~" (decode unreserved)
is_valid("hello%20world"); // true
is_valid("hello%GG");      // false
```

### Display wrapper & trait

```rust
use pct::{Encoded, PercentEncode};

// Inline formatting
let url = format!("https://example.com/{}", Encoded("foo/bar"));
assert_eq!(url, "https://example.com/foo%2Fbar");

// Extension trait
assert_eq!("hello world".percent_encode(), "hello%20world");
assert_eq!("hello%20world".percent_decode(), "hello world");
```

### IRI → URI (optional)

Enable the `iri` feature to encode non-ASCII characters in IRIs to valid URI percent-encoding:

```toml
[dependencies]
pct = { version = "0.1", features = ["iri"] }
```

```rust
use pct::encode_iri;

encode_iri("café"); // "caf%C3%A9"
```

## Custom encode sets

```rust
use pct::EncodeSet;

// Build your own by composing existing sets
const MY_SET: &EncodeSet = &EncodeSet::COMPONENT
    .remove(b'/')       // don't encode /
    .add(b'!');          // encode !

assert!(MY_SET.contains(b'!'));
assert!(!MY_SET.contains(b'/'));
```

## `no_std` usage

`pct` works with `no_std` out of the box — it only requires `alloc`:

```rust
// No special configuration needed — the crate is no_std by default.
// The optional "std" feature enables std::error::Error for DecodeError.
```

## Comparison with alternatives

| Feature | `pct` | `percent-encoding` | `urlencoding` |
|---------|-------|--------------------|---------------|
| Zero dependencies | ✅ | ✅ | ✅ |
| `no_std` + `alloc` | ✅ | ✅ | ❌ |
| Encodes bare `%` | ✅ | ❌ | ✅ |
| Idempotent encoding | ✅ | ❌ | ❌ |
| Form-urlencoded built-in | ✅ | ❌ | ❌ |
| Binary data support | ✅ | Partial | Partial |
| Predefined context sets | 6 | 2 | 0 |
| Multiple decode modes | 3 + bytes | 1 | 1 |
| Normalization | ✅ | ❌ | ❌ |
| Validation | ✅ | ❌ | ❌ |
| `Cow` zero-alloc path | ✅ | ✅ (iterator) | ❌ |
| IRI support | ✅ (opt-in) | ❌ | ❌ |

