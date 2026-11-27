//! gRPC's use of HTTP/2 headers: metadata, `grpc-timeout`, and the
//! percent-encoded `grpc-message` trailer.
//!
//! ```text
//! request headers   :method POST  :path /echo.Echo/Say  content-type application/grpc
//!                   te trailers  grpc-timeout 250m  x-user alice  token-bin AAEC
//! response headers  :status 200  content-type application/grpc
//! response trailers grpc-status 5  grpc-message no%20such%20user  x-request-id 7
//! ```
//!
//! Metadata names ending in `-bin` carry binary values, base64 on the wire;
//! PHP sees the raw bytes.

use std::time::Duration;

use base64::Engine;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use http::header::{HeaderMap, HeaderName, HeaderValue};
use percent_encoding::{AsciiSet, CONTROLS, percent_decode, percent_encode};
use phera_extension_api::{ArrayKey, PhpArray, Value};

/// Unpadded on the wire; either form is accepted.
const BASE64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_encode_padding(false)
        .with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// What `grpc-message` must escape besides bytes outside printable ASCII.
const MESSAGE: &AsciiSet = &CONTROLS.add(b'%');

/// Parses `grpc-timeout`: at most eight digits and a unit, `H` hours, `M`
/// minutes, `S` seconds, `m` milliseconds, `u` microseconds, `n` nanoseconds.
pub fn parse_timeout(value: &[u8]) -> Option<Duration> {
    let (&unit, digits) = value.split_last()?;
    if digits.is_empty() || digits.len() > 8 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let amount: u64 = std::str::from_utf8(digits).ok()?.parse().ok()?;
    Some(match unit {
        b'H' => Duration::from_secs(amount * 3600),
        b'M' => Duration::from_secs(amount * 60),
        b'S' => Duration::from_secs(amount),
        b'm' => Duration::from_millis(amount),
        b'u' => Duration::from_micros(amount),
        b'n' => Duration::from_nanos(amount),
        _ => return None,
    })
}

/// The shortest exact `grpc-timeout` for `timeout`, rounding up only when
/// eight digits of milliseconds are not enough.
pub fn format_timeout(timeout: Duration) -> String {
    const MAX: u128 = 99_999_999;
    let millis = timeout.as_millis();
    if !timeout.subsec_nanos().is_multiple_of(1_000_000) && timeout.as_nanos() <= MAX {
        return format!("{}n", timeout.as_nanos());
    }
    if millis <= MAX {
        return format!("{millis}m");
    }
    let seconds = timeout.as_nanos().div_ceil(1_000_000_000);
    if seconds <= MAX {
        return format!("{seconds}S");
    }
    format!("{}H", seconds.div_ceil(3600).min(MAX))
}

pub fn encode_message(message: &str) -> String {
    percent_encode(message.as_bytes(), MESSAGE).to_string()
}

pub fn decode_message(value: &[u8]) -> String {
    percent_decode(value).decode_utf8_lossy().into_owned()
}

/// Headers that belong to the protocol, not to the application.
pub fn is_reserved(name: &str) -> bool {
    name.starts_with("grpc-") || matches!(name, "content-type" | "te" | "user-agent")
}

/// A header's value as PHP sees it: `-bin` values decoded.
pub fn header_bytes(name: &str, value: &HeaderValue) -> Vec<u8> {
    if name.ends_with("-bin") {
        // Several binary values may arrive joined by commas.
        let mut out = Vec::new();
        for part in value.as_bytes().split(|&byte| byte == b',') {
            if let Ok(decoded) = BASE64.decode(part.trim_ascii()) {
                out.extend_from_slice(&decoded);
            }
        }
        out
    } else {
        value.as_bytes().to_vec()
    }
}

/// The application's headers as `name => value`. A name that appears more
/// than once has its values joined with `,` (binary ones concatenated).
pub fn to_array(headers: &HeaderMap) -> PhpArray {
    let mut array = PhpArray::new();
    for name in headers.keys() {
        let name = name.as_str();
        if is_reserved(name) {
            continue;
        }
        let mut joined: Vec<u8> = Vec::new();
        for (index, value) in headers.get_all(name).iter().enumerate() {
            if index > 0 && !name.ends_with("-bin") {
                joined.push(b',');
            }
            joined.extend_from_slice(&header_bytes(name, value));
        }
        array.insert(ArrayKey::str(name), Value::from(joined));
    }
    array
}

/// Adds metadata from PHP (`name => value` or `name => [values]`) to
/// outgoing headers, checking what HTTP/2 and gRPC allow.
pub fn append(headers: &mut HeaderMap, metadata: &PhpArray) -> Result<(), String> {
    for (key, value) in metadata.iter() {
        let name = match &key {
            ArrayKey::Int(number) => number.to_string(),
            ArrayKey::Interned(name) => name.to_string_lossy(),
            ArrayKey::String(name) => name.to_string_lossy(),
        };
        let value = value.deref();
        match &*value {
            Value::Array(values) => {
                for (_, value) in values.iter() {
                    insert(headers, &name, &value.deref())?;
                }
            }
            other => insert(headers, &name, other)?,
        }
    }
    Ok(())
}

/// Adds one metadata entry.
pub fn insert(headers: &mut HeaderMap, name: &str, value: &Value) -> Result<(), String> {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("grpc-") || lower.starts_with(':') {
        return Err(format!("metadata name \"{name}\" is reserved"));
    }
    let valid_name = !lower.is_empty()
        && lower.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.".contains(&byte)
        });
    let header_name = HeaderName::from_bytes(lower.as_bytes())
        .ok()
        .filter(|_| valid_name)
        .ok_or_else(|| format!("invalid metadata name \"{name}\""))?;
    let bytes: Vec<u8> = match value {
        Value::Int(number) => number.to_string().into_bytes(),
        other => other
            .as_str_bytes()
            .ok_or_else(|| format!("metadata \"{name}\" must be a string"))?
            .to_vec(),
    };
    let header_value = if lower.ends_with("-bin") {
        HeaderValue::from_str(&BASE64.encode(&bytes))
    } else {
        if !bytes.iter().all(|&byte| (0x20..0x7f).contains(&byte)) {
            return Err(format!(
                "metadata \"{name}\" must be printable ASCII; use a name ending in -bin for bytes"
            ));
        }
        HeaderValue::from_bytes(&bytes)
    }
    .map_err(|_| format!("invalid value for metadata \"{name}\""))?;
    headers.append(header_name, header_value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeouts_parse_every_unit() {
        assert_eq!(parse_timeout(b"1H"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_timeout(b"2M"), Some(Duration::from_secs(120)));
        assert_eq!(parse_timeout(b"3S"), Some(Duration::from_secs(3)));
        assert_eq!(parse_timeout(b"250m"), Some(Duration::from_millis(250)));
        assert_eq!(parse_timeout(b"7u"), Some(Duration::from_micros(7)));
        assert_eq!(parse_timeout(b"9n"), Some(Duration::from_nanos(9)));
        assert_eq!(parse_timeout(b"123456789m"), None, "nine digits");
        assert_eq!(parse_timeout(b"m"), None);
        assert_eq!(parse_timeout(b"5x"), None);
        assert_eq!(parse_timeout(b"-5m"), None);
    }

    #[test]
    fn timeouts_format_within_eight_digits() {
        assert_eq!(format_timeout(Duration::from_millis(250)), "250m");
        assert_eq!(format_timeout(Duration::from_micros(1500)), "1500000n");
        assert_eq!(format_timeout(Duration::from_secs(200_000)), "200000S");
        for timeout in [
            Duration::from_millis(1),
            Duration::from_secs(3600 * 24 * 365),
        ] {
            let text = format_timeout(timeout);
            assert!(text.len() <= 9, "{text}");
            assert!(parse_timeout(text.as_bytes()).unwrap() >= timeout);
        }
    }

    #[test]
    fn messages_are_percent_encoded() {
        let encoded = encode_message("no user 100% \"ok\" é\n");
        assert_eq!(encoded, "no user 100%25 \"ok\" %C3%A9%0A");
        assert_eq!(
            decode_message(encoded.as_bytes()),
            "no user 100% \"ok\" é\n"
        );
    }

    #[test]
    fn metadata_round_trips_text_and_binary_values() {
        let mut headers = HeaderMap::new();
        insert(&mut headers, "X-User", &Value::from("alice")).unwrap();
        insert(
            &mut headers,
            "token-bin",
            &Value::from(&b"\x00\x01\xff"[..]),
        )
        .unwrap();
        assert_eq!(headers.get("token-bin").unwrap(), "AAH/");
        headers.insert("grpc-status", HeaderValue::from_static("0"));
        let array = to_array(&headers);
        assert_eq!(array.len(), 2);
        assert_eq!(
            array
                .get(&ArrayKey::str("token-bin"))
                .unwrap()
                .as_str_bytes(),
            Some(&b"\x00\x01\xff"[..])
        );
        assert!(insert(&mut headers, "grpc-x", &Value::from("v")).is_err());
        assert!(insert(&mut headers, "bad name", &Value::from("v")).is_err());
        assert!(insert(&mut headers, "x", &Value::from("line\nbreak")).is_err());
    }
}
