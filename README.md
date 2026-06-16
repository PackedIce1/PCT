# pct

Percent-encoding and decoding for URLs — pure Rust, zero dependencies, `no_std` with **optional** `alloc`, **optional** SIMD, zero-allocation `Cow` API.

A modern alternative to the `percent-encoding` crate that fixes its long-standing pain points, adds competitive performance via SIMD, and runs in environments no other percent-encoding crate can (kernels, bootloaders, microcontrollers without a heap).

## What's new in 0.2

- **`alloc` is now optional.** Disable default features to use only the allocation-free core (`EncodeSet`, `is_valid`, scanning, length pre-computation) in `no_std` environments without a heap. The full `Cow`-returning API is still available by default.
- **SIMD acceleration** via `core::simd` (nightly `portable_simd`). The no-op fast path scans 32 bytes per cycle on AVX2 / NEON targets, bringing the "already-canonical input" cost close to the ~1.4 ns achieved by `percent-encoding`.
- **Tighter `Cow` fast path.** All `encode`/`decode` entry points now route through a shared SIMD-accelerated scanner, so the `Cow::Borrowed` case is faster than ever.

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
| `alloc` required | Always allocates | `alloc` is **optional** — core API works in kernels/embedded |
| SIMD acceleration | None | Optional `simd` feature via `core::simd` |

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

### `no_std` without `alloc`

For environments without a heap (kernels, microcontrollers, boot loaders):

```toml
[dependencies]
pct = { version = "0.2", default-features = false }
```

The following APIs remain available without `alloc`:

- `EncodeSet` and all predefined constants (`COMPONENT`, `PATH`, `QUERY`, `FRAGMENT`, `CONTROLS`, `NON_ALPHANUMERIC`)
- `is_hex()`, `hex_val()`, `HEX_UPPER`, `HEX_LOWER`
- `is_valid()`, `is_valid_bytes()`
- `find_first_byte()`, `find_first_byte_raw()`, `find_first_byte_idempotent()`
- `needs_encoding_raw()`, `needs_encoding_idempotent()`
- `encoded_len_raw()`, `encoded_len_idempotent()`

You can use these to pre-validate input, compute output buffer sizes, or write your own encoding into a fixed-size buffer — all without touching the heap.

### SIMD acceleration

Enable on nightly Rust:

```toml
[dependencies]
pct = { version = "0.2", features = ["simd"] }
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
pct = { version = "0.2", features = ["iri"] }
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
| Predefined context sets | 6 | 2 | 0 |
| Multiple decode modes | 3 + bytes | 1 | 1 |
| Normalization | ✅ | ❌ | ❌ |
| Validation | ✅ | ❌ | ❌ |
| `Cow` zero-alloc path | ✅ | ✅ (iterator) | ❌ |
| IRI support | ✅ (opt-in) | ❌ | ❌ |

## License

MPL-2.0
