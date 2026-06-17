# pct

Percent-encoding and decoding for URLs — pure Rust, zero dependencies, `no_std` with **optional** `alloc`, **optional** SIMD, zero-allocation `Cow` API, compile-time encoding, streaming iterators, and explicit mode API.

A modern alternative to the `percent-encoding` crate that fixes its long-standing pain points, adds competitive performance via SIMD, and runs in environments no other percent-encoding crate can (kernels, bootloaders, microcontrollers without a heap).

## What's new in 0.3

- **Compile-time encoding** — `const_encode!("Hello World")` produces `"Hello%20World"` at compile time with **0 ns runtime cost**. The encoded string is embedded directly in the binary.
- **Streaming/iterator API** — `EncodedBytes` and `DecodedChars` yield encoded bytes or decoded characters one at a time with **zero heap allocation**, enabling processing of massive files without loading them into RAM.
- **Explicit mode API** — `Pct::encode_rfc3986()`, `Pct::encode_whatwg()`, `Pct::encode_html_form()` make the encoding standard unambiguous, preventing bugs from applying the wrong type of encoding.
- **WHATWG URL Standard encode set** — New `EncodeSet::WHATWG` preserves URL code points like `!`, `'`, `(`, `)`, `*`, `+`, matching browser behaviour.
- **`alloc` is optional.** Disable default features to use only the allocation-free core (`EncodeSet`, `is_valid`, scanning, length pre-computation) in `no_std` environments without a heap. The full `Cow`-returning API is still available by default.
- **SIMD acceleration** via `core::simd` (nightly `portable_simd`). The no-op fast path scans 32 bytes per cycle on AVX2 / NEON targets, bringing the "already-canonical input" cost close to the ~1.4 ns achieved by the `percent-encoding` crate.
- **Tighter `Cow` fast path.** All `encode`/`decode` entry points now route through a shared SIMD-accelerated scanner, so the `Cow::Borrowed` case is faster than ever.

## Why `pct`?

The `percent-encoding` crate is widely used but has several well-known issues that have remained open for years. `pct` fixes all of them:

| Issue | `percent-encoding` | `pct` |
|-------|--------------------|-------|
| [#503](https://github.com/servo/rust-url/issues/503) — bare `%` not encoded | `%` is left as-is → broken round-trips | `encode("100%")` → `"100%25"` |
| [#416](https://github.com/servo/rust-url/issues/416) / [#482](https://github.com/servo/rust-url/issues/482) — `+` ambiguity | `+` is not encoded; no form-urlencoded support | `encode("a+b")` → `"a%2Bb"`; dedicated `encode_form`/`decode_form` |
| Few predefined sets | Only `CONTROLS` and `NON_ALPHANUMERIC` | `COMPONENT`, `PATH`, `QUERY`, `FRAGMENT`, `CONTROLS`, `NON_ALPHANUMERIC`, `WHATWG` |
| No form-urlencoded support | Requires a separate crate | Built-in `encode_form` / `decode_form` |
| No binary data support | Requires low-level API | `encode_bytes(&[u8])` works directly |
| Idempotent encoding | Double-encoding is easy | `encode("foo%20bar")` is a no-op |
| `alloc` required | Always allocates | `alloc` is **optional** — core API works in kernels/embedded |
| SIMD acceleration | None | Optional `simd` feature via `core::simd` |
| No compile-time encoding | Runtime only | `const_encode!("Hello")` → `"Hello"` at 0 ns |
| No streaming API | Returns `String` | `EncodedBytes` / `DecodedChars` — zero-allocation iterators |
| Ambiguous "URL encoding" | Users must know which function | `Pct::encode_rfc3986()` / `Pct::encode_whatwg()` / `Pct::encode_html_form()` |

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

| Feature   | Default | Description |
|-----------|---------|-------------|
| `alloc`   | ✅ on   | Enables allocation-backed APIs (`encode`, `decode`, `encode_form`, etc.) returning `Cow<'_, str>`. |
| `std`     | ❌ off  | Enables `std::error::Error` for `DecodeError`. Implies `alloc`. |
| `iri`     | ❌ off  | Enables `encode_iri()` for internationalized resource identifiers. Implies `alloc`. |
| `simd`    | ❌ off  | SIMD acceleration via `core::simd` (requires nightly Rust). Independent of `alloc`. |

### Compile-time encoding

Encode strings at compile time with **zero runtime cost**:

```rust
use pct::const_encode;

const ENCODED: &str = const_encode!("Hello World");
assert_eq!(ENCODED, "Hello%20World");

// Custom encode set
const PATH: &str = const_encode!("a/b c", &pct::EncodeSet::PATH);
assert_eq!(PATH, "a/b%20c");

// UTF-8 input
const UNICODE: &str = const_encode!("café");
assert_eq!(UNICODE, "caf%C3%A9");
```

The input is limited to 1024 bytes (`MAX_CONST_INPUT_LEN`). The encoded result is embedded directly in the binary as a `&'static str`.

### Streaming / zero-allocation iterators

Process massive inputs without loading them into RAM:

```rust
use pct::{EncodedBytes, DecodedChars, EncodeSet};

// Encode byte-by-byte — zero heap allocation
let mut encoder = EncodedBytes::new("hello world", &EncodeSet::COMPONENT);
let encoded: Vec<u8> = encoder.collect();
assert_eq!(String::from_utf8(encoded).unwrap(), "hello%20world");

// Decode char-by-char — zero heap allocation, handles multi-byte UTF-8
let mut decoder = DecodedChars::new("caf%C3%A9");
assert_eq!(decoder.next(), Some('c'));
assert_eq!(decoder.next(), Some('a'));
assert_eq!(decoder.next(), Some('f'));
assert_eq!(decoder.next(), Some('é'));
assert_eq!(decoder.next(), None);
```

Both iterators work in `#![no_std]` without `alloc`.

### Explicit mode API

Make the encoding standard unambiguous with `Pct`:

```rust
use pct::Pct;

// RFC 3986: strict, for URI components (space → %20, + → %2B)
assert_eq!(Pct::encode_rfc3986("hello world"), "hello%20world");
assert_eq!(Pct::encode_rfc3986("a+b"), "a%2Bb");

// WHATWG URL Standard: more permissive (allows !, ', (, ), *, +, etc.)
assert_eq!(Pct::encode_whatwg("keep'safe"), "keep'safe");
assert_eq!(Pct::encode_whatwg("a+b"), "a+b");

// HTML Form: space → +, literal + → %2B
assert_eq!(Pct::encode_html_form("hello world"), "hello+world");
assert_eq!(Pct::encode_html_form("a+b"), "a%2Bb");

// Decoding modes too
assert_eq!(Pct::decode_rfc3986("hello%20world"), "hello world");
assert_eq!(Pct::decode_html_form("hello+world"), "hello world");
```

| Context | Method | Space | `+` sign |
|---------|--------|-------|----------|
| URL path / component | `Pct::encode_rfc3986()` | `%20` | `%2B` |
| WHATWG URL parsing | `Pct::encode_whatwg()` | `%20` | passed through |
| HTML `<form>` submission | `Pct::encode_html_form()` | `+` | `%2B` |

### `no_std` without `alloc`

For environments without a heap (kernels, microcontrollers, boot loaders):

```toml
[dependencies]
pct = { version = "0.3", default-features = false }
```

The following APIs remain available without `alloc`:

- `EncodeSet` and all predefined constants (`COMPONENT`, `PATH`, `QUERY`, `FRAGMENT`, `CONTROLS`, `NON_ALPHANUMERIC`, `WHATWG`)
- `is_hex()`, `hex_val()`, `HEX_UPPER`, `HEX_LOWER`
- `is_valid()`, `is_valid_bytes()`
- `find_first_byte()`, `find_first_byte_raw()`, `find_first_byte_idempotent()`
- `needs_encoding_raw()`, `needs_encoding_idempotent()`
- `encoded_len_raw()`, `encoded_len_idempotent()`
- `EncodedBytes`, `DecodedChars` (streaming iterators)
- `const_encode_to_buf()`, `const_encoded_len()` (const helpers)
- `const_encode!` (compile-time encoding macro)

You can use these to pre-validate input, compute output buffer sizes, stream-encode/decode data, or write your own encoding into a fixed-size buffer — all without touching the heap.

### SIMD acceleration

Enable on nightly Rust:

```toml
[dependencies]
pct = { version = "0.3", features = ["simd"] }
```

This enables `#![feature(portable_simd)]` internally and dispatches the no-op fast path to `core::simd`-accelerated implementations. Already-canonical inputs (the common case for valid URLs) are scanned 32 bytes per cycle on AVX2 / NEON targets, bringing the no-op cost close to the ~1.4 ns achieved by the `percent-encoding` crate.

The `simd` feature is independent of `alloc` — you can combine `simd` + `no_std` + no `alloc` for high-performance scanning in a kernel.

## API overview

### Encoding (requires `alloc`)

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

### Convenience functions (requires `alloc`)

```rust
use pct::{encode_for_path, encode_for_query, encode_for_fragment, encode_for_component};

encode_for_path("a/b c");       // "a/b%20c"
encode_for_query("k=v&x=1 y");  // "k=v&x=1%20y"
encode_for_fragment("a#b c");   // "a#b%20c"
encode_for_component("a/b?c");  // "a%2Fb%3Fc"
```

### Decoding (requires `alloc`)

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

### Form-urlencoded (requires `alloc`)

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

### Normalization & validation (always available)

```rust
use pct::{normalize, is_valid};

normalize("%2f%2F");     // "%2F%2F" (uppercase hex)
normalize("%7E");        // "~" (decode unreserved)
is_valid("hello%20world"); // true
is_valid("hello%GG");      // false
```

### Allocation-free scanning (always available)

```rust
use pct::{EncodeSet, needs_encoding_idempotent, encoded_len_idempotent};

let set = EncodeSet::COMPONENT;
let input = b"hello%20world";

// Check if encoding is needed — no allocation, no_std friendly
if !needs_encoding_idempotent(input, &set) {
    // Input is already canonical, can be used as-is
}

// Pre-compute the encoded output length — useful for fixed-size buffers
let len = encoded_len_idempotent(input, &set);
```

### Display wrapper & trait (requires `alloc`)

```rust
use pct::{Encoded, PercentEncode};

// Inline formatting
let url = format!("https://example.com/{}", Encoded("foo/bar"));
assert_eq!(url, "https://example.com/foo%2Fbar");

// Extension trait
assert_eq!("hello world".percent_encode(), "hello%20world");
assert_eq!("hello%20world".percent_decode(), "hello world");
```

### IRI → URI (optional, requires `alloc` + `iri`)

Enable the `iri` feature to encode non-ASCII characters in IRIs to valid URI percent-encoding:

```toml
[dependencies]
pct = { version = "0.3", features = ["iri"] }
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

## Comparison with alternatives

| Feature | `pct` | `percent-encoding` | `urlencoding` |
|---------|-------|--------------------|---------------|
| Zero dependencies | ✅ | ✅ | ✅ |
| `no_std` + optional `alloc` | ✅ | `alloc` always required | ❌ |
| SIMD acceleration | ✅ (opt-in, nightly) | ❌ | ❌ |
| Encodes bare `%` | ✅ | ❌ | ✅ |
| Idempotent encoding | ✅ | ❌ | ❌ |
| Form-urlencoded built-in | ✅ | ❌ | ❌ |
| Binary data support | ✅ | Partial | Partial |
| Predefined context sets | 7 | 2 | 0 |
| Multiple decode modes | 3 + bytes | 1 | 1 |
| Normalization | ✅ | ❌ | ❌ |
| Validation | ✅ | ❌ | ❌ |
| `Cow` zero-alloc path | ✅ | ✅ (iterator) | ❌ |
| IRI support | ✅ (opt-in) | ❌ | ❌ |
| Compile-time encoding | ✅ (`const_encode!`) | ❌ | ❌ |
| Streaming iterators | ✅ (`EncodedBytes` / `DecodedChars`) | ✅ (lazy iterator) | ❌ |
| Explicit mode API | ✅ (`Pct`) | ❌ | ❌ |
| WHATWG encode set | ✅ (`EncodeSet::WHATWG`) | ❌ | ❌ |

## License

MPL-2.0