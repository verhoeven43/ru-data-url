//! Parse `data:` URLs into their MIME type, base64 flag, and decoded payload.
//!
//! The parser follows RFC 2397. It is deliberately strict about the parts
//! that are easy to get subtly wrong (percent-decoding of the media type,
//! the base64 alphabet, padding) and lenient about the parts where the
//! spec is silent or where real-world producers diverge (whitespace inside
//! base64 payloads, an empty media type).

use std::fmt;

/// Errors that can occur while parsing a data URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataUrlError {
    /// The input did not start with the literal `data:` scheme.
    NotADataUrl,
    /// The media type contained a `%` sequence that was not valid hex.
    InvalidPercentEncoding,
    /// A `%` appeared at the very end of the media type with no digits.
    TruncatedPercentEncoding,
    /// The payload was marked base64 but contained characters outside the
    /// base64 alphabet (after ignoring ASCII whitespace).
    InvalidBase64,
    /// The base64 payload had a length that cannot be valid base64.
    InvalidBase64Length,
}

impl fmt::Display for DataUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            DataUrlError::NotADataUrl => "input does not start with `data:`",
            DataUrlError::InvalidPercentEncoding => "invalid percent-encoding in media type",
            DataUrlError::TruncatedPercentEncoding => "truncated percent-encoding in media type",
            DataUrlError::InvalidBase64 => "payload is not valid base64",
            DataUrlError::InvalidBase64Length => "base64 payload has an impossible length",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for DataUrlError {}

/// A parsed data URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataUrl {
    /// The media type, e.g. `text/plain;charset=utf-8`.
    ///
    /// An omitted media type is reported as `text/plain;charset=US-ASCII`,
    /// which is the default RFC 2397 assigns to `data:,`.
    pub mime_type: String,
    /// Whether the payload was base64-encoded.
    pub is_base64: bool,
    /// The decoded payload bytes.
    pub data: Vec<u8>,
}

impl DataUrl {
    /// Parse a `data:` URL.
    ///
    /// The returned `data` field always holds the *decoded* bytes: for a
    /// base64 URL it is the decoded binary, and for a plain URL it is the
    /// percent-decoded text.
    pub fn parse(input: &str) -> Result<DataUrl, DataUrlError> {
        let rest = input.strip_prefix("data:").ok_or(DataUrlError::NotADataUrl)?;

        // The media type and the payload are separated by the first comma.
        // Commas inside the payload are fine; commas inside the media type
        // would have to be percent-encoded, so splitting on the first one is
        // correct.
        let (meta, payload) = match rest.find(',') {
            Some(idx) => (&rest[..idx], &rest[idx + 1..]),
            // RFC 2397 requires the comma. Treat a missing comma as an empty
            // media type with an empty payload rather than erroring, because
            // `data:` with nothing after it is unambiguous.
            None => (rest, ""),
        };

        let (mime_type, is_base64) = parse_meta(meta)?;
        let data = if is_base64 {
            decode_base64(payload)?
        } else {
            decode_percent_bytes(payload)?
        };

        Ok(DataUrl {
            mime_type,
            is_base64,
            data,
        })
    }
}

/// Split the metadata section into a media type and the base64 flag.
fn parse_meta(meta: &str) -> Result<(String, bool), DataUrlError> {
    // The base64 flag is the final `;base64` marker, case-insensitively.
    // We look at the end of the metadata so that a media type parameter
    // whose value happens to be `base64` is not mistaken for the flag.
    let trimmed = meta.trim_end_matches(';');
    let (type_part, is_base64) = match trimmed.rfind(';') {
        Some(idx) => {
            let tail = &trimmed[idx + 1..];
            if tail.eq_ignore_ascii_case("base64") {
                (&trimmed[..idx], true)
            } else {
                (trimmed, false)
            }
        }
        None => {
            if trimmed.eq_ignore_ascii_case("base64") {
                ("", true)
            } else {
                (trimmed, false)
            }
        }
    };

    let decoded = decode_percent(type_part)?;
    let mime_type = if decoded.is_empty() {
        // RFC 2397: an omitted media type defaults to this exact string.
        "text/plain;charset=US-ASCII".to_string()
    } else {
        decoded
    };

    Ok((mime_type, is_base64))
}

/// Percent-decode a string into a `String`.
///
/// Data URLs encode arbitrary bytes in the media type with `%XX`. We decode
/// to a `String` because the media type is textual; a `%XX` that decodes to
/// a non-UTF-8 byte is replaced with U+FFFD, which matches how browsers
/// surface such malformed types.
fn decode_percent(input: &str) -> Result<String, DataUrlError> {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(DataUrlError::TruncatedPercentEncoding);
            }
            let hi = hex_val(bytes[i + 1]).ok_or(DataUrlError::InvalidPercentEncoding)?;
            let lo = hex_val(bytes[i + 2]).ok_or(DataUrlError::InvalidPercentEncoding)?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Decode a percent-encoded payload into raw bytes.
///
/// Unlike the media type this must preserve arbitrary bytes, so we return a
/// `Vec<u8>` and never go through `String`.
fn decode_percent_bytes(input: &str) -> Result<Vec<u8>, DataUrlError> {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(DataUrlError::TruncatedPercentEncoding);
            }
            let hi = hex_val(bytes[i + 1]).ok_or(DataUrlError::InvalidPercentEncoding)?;
            let lo = hex_val(bytes[i + 2]).ok_or(DataUrlError::InvalidPercentEncoding)?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Ok(out)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Decode a base64 payload, ignoring ASCII whitespace.
///
/// Whitespace is tolerated because newlines in base64 data URLs are common
/// in the wild even though the spec does not allow them.
fn decode_base64(input: &str) -> Result<Vec<u8>, DataUrlError> {
    let mut out: Vec<u8> = Vec::with_capacity(input.len() / 4 * 3);
    let mut quad = [0u8; 4];
    let mut quad_len = 0usize;
    let mut pad = 0usize;

    for &b in input.as_bytes() {
        if b.is_ascii_whitespace() {
            continue;
        }
        if b == b'=' {
            // Padding must come after the data and only in the last quad.
            pad += 1;
            if pad > 2 {
                return Err(DataUrlError::InvalidBase64);
            }
            quad[quad_len] = 0;
            quad_len += 1;
        } else {
            if pad > 0 {
                // A data character after padding is malformed.
                return Err(DataUrlError::InvalidBase64);
            }
            quad[quad_len] = base64_val(b).ok_or(DataUrlError::InvalidBase64)?;
            quad_len += 1;
        }
        if quad_len == 4 {
            out.push((quad[0] << 2) | (quad[1] >> 4));
            if pad < 2 {
                out.push((quad[1] << 4) | (quad[2] >> 2));
            }
            if pad < 1 {
                out.push((quad[2] << 6) | quad[3]);
            }
            quad_len = 0;
        }
    }

    // A trailing partial quad is only valid if it is exactly 2 or 3
    // characters of data (i.e. 1 or 2 output bytes) with no padding, or a
    // fully padded quad which we already handled above.
    match quad_len {
        0 => {}
        2 => {
            out.push((quad[0] << 2) | (quad[1] >> 4));
        }
        3 => {
            out.push((quad[0] << 2) | (quad[1] >> 4));
            out.push((quad[1] << 4) | (quad[2] >> 2));
        }
        _ => return Err(DataUrlError::InvalidBase64Length),
    }

    Ok(out)
}

fn base64_val(b: u8) -> Option<u8> {
    match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_default_media_type() {
        let d = DataUrl::parse("data:,Hello%2C%20World").unwrap();
        assert_eq!(d.mime_type, "text/plain;charset=US-ASCII");
        assert!(!d.is_base64);
        assert_eq!(d.data, b"Hello, World");
    }

    #[test]
    fn explicit_media_type() {
        let d = DataUrl::parse("data:text/plain;charset=utf-8,hello").unwrap();
        assert_eq!(d.mime_type, "text/plain;charset=utf-8");
        assert!(!d.is_base64);
        assert_eq!(d.data, b"hello");
    }

    #[test]
    fn base64_payload() {
        // "Man" is the canonical base64 example.
        let d = DataUrl::parse("data:text/plain;base64,TWFu").unwrap();
        assert_eq!(d.mime_type, "text/plain");
        assert!(d.is_base64);
        assert_eq!(d.data, b"Man");
    }

    #[test]
    fn base64_with_padding() {
        let d = DataUrl::parse("data:application/octet-stream;base64,TWE=").unwrap();
        assert_eq!(d.data, b"Ma");
    }

    #[test]
    fn base64_with_whitespace() {
        let d = DataUrl::parse("data:;base64,TW Fu\n").unwrap();
        assert!(d.is_base64);
        assert_eq!(d.data, b"Man");
    }

    #[test]
    fn base64_flag_is_case_insensitive() {
        let d = DataUrl::parse("data:text/plain;BASE64,TWFu").unwrap();
        assert!(d.is_base64);
        assert_eq!(d.data, b"Man");
    }

    #[test]
    fn omitted_media_type_with_base64() {
        let d = DataUrl::parse("data:;base64,TWFu").unwrap();
        assert_eq!(d.mime_type, "text/plain;charset=US-ASCII");
        assert_eq!(d.data, b"Man");
    }

    #[test]
    fn percent_encoded_media_type() {
        let d = DataUrl::parse("data:text%2Fplain,hi").unwrap();
        assert_eq!(d.mime_type, "text/plain");
        assert_eq!(d.data, b"hi");
    }

    #[test]
    fn plus_is_literal_not_space() {
        // In a data URL `+` is a literal plus; only `%20` is a space.
        let d = DataUrl::parse("data:,a+b").unwrap();
        assert_eq!(d.data, b"a+b");
    }

    #[test]
    fn missing_scheme_is_an_error() {
        assert_eq!(DataUrl::parse("http://example"), Err(DataUrlError::NotADataUrl));
    }

    #[test]
    fn truncated_percent_is_an_error() {
        assert_eq!(
            DataUrl::parse("data:,abc%"),
            Err(DataUrlError::TruncatedPercentEncoding)
        );
    }

    #[test]
    fn invalid_percent_hex_is_an_error() {
        assert_eq!(
            DataUrl::parse("data:,abc%zz"),
            Err(DataUrlError::InvalidPercentEncoding)
        );
    }

    #[test]
    fn invalid_base64_character_is_an_error() {
        assert_eq!(
            DataUrl::parse("data:;base64,TW*u"),
            Err(DataUrlError::InvalidBase64)
        );
    }

    #[test]
    fn base64_with_one_character_is_an_error() {
        assert_eq!(
            DataUrl::parse("data:;base64,T"),
            Err(DataUrlError::InvalidBase64Length)
        );
    }

    #[test]
    fn empty_payload_is_allowed() {
        let d = DataUrl::parse("data:").unwrap();
        assert_eq!(d.mime_type, "text/plain;charset=US-ASCII");
        assert!(!d.is_base64);
        assert!(d.data.is_empty());
    }

    #[test]
    fn comma_inside_payload_is_preserved() {
        let d = DataUrl::parse("data:,a,b,c").unwrap();
        assert_eq!(d.data, b"a,b,c");
    }

    #[test]
    fn arbitrary_bytes_round_trip() {
        let d = DataUrl::parse("data:application/octet-stream,%00%01%FE%FF").unwrap();
        assert_eq!(d.data, vec![0x00, 0x01, 0xFE, 0xFF]);
    }
}
