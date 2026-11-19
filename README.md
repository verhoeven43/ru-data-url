# ru-data-url

Parse `data:` URLs into their MIME type, a base64 flag, and the decoded payload bytes.

```rust
use ru_data_url::DataUrl;

let d = DataUrl::parse("data:text/plain;charset=utf-8;base64,SGVsbG8=").unwrap();
assert_eq!(d.mime_type, "text/plain;charset=utf-8");
assert!(d.is_base64);
assert_eq!(d.data, b"Hello");
```

## Why this exists

A data URL packs three things into one string: a media type, a flag telling you
whether the payload is base64, and the payload itself. Splitting that string
naively on `,` and `;` breaks the moment a media type contains a percent-encoded
character or a payload contains a comma, both of which are legal. This crate
does the split in the right order and decodes the payload for you.

The trade-off: `parse` returns a `Result` and is strict about malformed input
rather than guessing. A truncated `%` or a base64 payload containing a stray
`*` is an error, not a best-effort decode. The one place it is deliberately
lenient is whitespace inside base64 payloads, which is common in the wild and
harmless to ignore.

## The awkward edge

`+` is a literal plus in a data URL, not a space. Only `%20` (or a raw space)
is a space. If you are used to form-encoded query strings this will surprise
you, and the test suite pins the behaviour.

An omitted media type is reported as `text/plain;charset=US-ASCII`, the default
RFC 2397 assigns to `data:,`. That means `data:,hi` and
`data:text/plain;charset=US-ASCII,hi` produce the same `mime_type`.

## API

- `DataUrl::parse(&str) -> Result<DataUrl, DataUrlError>`
- `DataUrl { mime_type: String, is_base64: bool, data: Vec<u8> }`
- `DataUrlError::{NotADataUrl, InvalidPercentEncoding, TruncatedPercentEncoding, InvalidBase64, InvalidBase64Length}`
