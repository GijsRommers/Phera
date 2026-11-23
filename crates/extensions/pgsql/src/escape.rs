//! `pg_escape_literal($db, $text)` quotes a PostgreSQL string literal.

use crate::connection;
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Value, args};

pub const STRING: usize = 0;
pub const LITERAL: usize = 1;
pub const IDENTIFIER: usize = 2;
pub const BYTEA: usize = 3;

pub fn escape<const OP: usize>(
    c: &mut dyn Context,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let explicit = args::object(&a, 0).is_some();
        let value = args::bytes(&a, usize::from(explicit));
        let call = match OP {
            STRING => "pg_escape_string",
            LITERAL => "pg_escape_literal",
            IDENTIFIER => "pg_escape_identifier",
            _ => "pg_escape_bytea",
        };
        let state = if explicit {
            Some(connection::argument(c, args::value(&a, 0), call)?)
        } else {
            c.deprecated(format!(
                "{call}(): Automatic fetching of PostgreSQL connection is deprecated"
            ));
            connection::default_handle(c)?
        };
        let standard = if let Some(state) = state {
            let lease = connection::lease(&state).await?;
            if matches!(OP, STRING | BYTEA) {
                connection::run(&lease, |driver| {
                    Box::pin(
                        async move { driver.parameter_status("standard_conforming_strings").await },
                    )
                })
                .await
                .ok()
                .flatten()
                .as_deref()
                    != Some("off")
            } else {
                true
            }
        } else {
            match OP {
                STRING => false,
                BYTEA => return Ok(Value::from(escape_legacy_bytea(value))),
                _ => return Err(NativeError::error("No PostgreSQL connection opened yet")),
            }
        };
        if OP == BYTEA {
            return Ok(Value::from(escape_bytea(value, standard)));
        }
        let bytes = value.split(|byte| *byte == 0).next().unwrap_or_default();
        if matches!(OP, LITERAL | IDENTIFIER) && std::str::from_utf8(bytes).is_err() {
            return Ok(Value::Bool(false));
        }
        let quote = if OP == IDENTIFIER { b'"' } else { b'\'' };
        let escaping = OP == LITERAL && bytes.contains(&b'\\') || OP == STRING && !standard;
        let mut escaped = Vec::new();
        if OP == LITERAL && escaping {
            escaped.extend_from_slice(b" E");
        }
        if OP != STRING {
            escaped.push(quote);
        }
        for &byte in bytes {
            if byte == quote || escaping && byte == b'\\' {
                escaped.push(byte);
            }
            escaped.push(byte);
        }
        if OP != STRING {
            escaped.push(quote);
        }
        Ok(Value::from(escaped))
    })
}

fn escape_bytea(bytes: &[u8], standard: bool) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(bytes.len().saturating_mul(2).saturating_add(3));
    encoded.extend_from_slice(if standard { b"\\x" } else { b"\\\\x" });
    const HEX: &[u8] = b"0123456789abcdef";
    for byte in bytes {
        encoded.push(HEX[usize::from(byte >> 4)]);
        encoded.push(HEX[usize::from(byte & 15)]);
    }
    encoded
}

fn escape_legacy_bytea(bytes: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::new();
    for &byte in bytes {
        match byte {
            b'\'' => encoded.extend_from_slice(b"''"),
            b'\\' => encoded.extend_from_slice(b"\\\\\\\\"),
            0x20..=0x7e => encoded.push(byte),
            _ => encoded.extend_from_slice(&[
                b'\\',
                b'\\',
                b'0' + (byte >> 6),
                b'0' + (byte >> 3 & 7),
                b'0' + (byte & 7),
            ]),
        }
    }
    encoded
}

pub fn unescape_bytea(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let bytes = args::bytes(a, 0);
    let mut output = Vec::new();
    if let Some(hex) = bytes.strip_prefix(b"\\x") {
        let mut digits = hex
            .iter()
            .copied()
            .filter(|byte| !byte.is_ascii_whitespace());
        while let (Some(high), Some(low)) = (digits.next(), digits.next()) {
            if let (Some(high), Some(low)) =
                ((high as char).to_digit(16), (low as char).to_digit(16))
            {
                output.push((high * 16 + low) as u8);
            }
        }
        return Ok(Value::from(output));
    }
    let mut bytes = bytes
        .iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .peekable();
    while let Some(byte) = bytes.next() {
        if byte != b'\\' {
            output.push(byte);
            continue;
        }
        if bytes.next_if_eq(&b'\\').is_some() {
            output.push(b'\\');
            continue;
        }
        let octal = bytes.peek().is_some_and(|byte| matches!(byte, b'0'..=b'3'));
        if octal {
            let first = bytes.next().unwrap_or(b'0');
            let second = bytes.next_if(|byte| matches!(byte, b'0'..=b'7'));
            let third = bytes.next_if(|byte| matches!(byte, b'0'..=b'7'));
            if let (Some(second), Some(third)) = (second, third) {
                output.push((first - b'0') * 64 + (second - b'0') * 8 + (third - b'0'));
            } else {
                output.push(first);
                output.extend(second);
                output.extend(third);
            }
        }
    }
    Ok(Value::from(output))
}

pub fn identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub fn table_name(name: &str) -> String {
    name.split('.')
        .map(|part| {
            identifier(
                &part
                    .strip_prefix('"')
                    .and_then(|p| p.strip_suffix('"'))
                    .unwrap_or(part)
                    .replace("\"\"", "\""),
            )
        })
        .collect::<Vec<_>>()
        .join(".")
}

pub fn literal(text: &str) -> String {
    let text = text.split('\0').next().unwrap_or_default();
    let body = text.replace('\'', "''");
    if body.contains('\\') {
        format!("E'{}'", body.replace('\\', "\\\\"))
    } else {
        format!("'{body}'")
    }
}
