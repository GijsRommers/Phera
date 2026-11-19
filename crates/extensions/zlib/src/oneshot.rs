//! Compress or decode a complete byte string, with PHP's bounded inflate rounds.

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use crate::deflate::{Encoder, Flush};
use crate::format::Encoding;
use crate::inflate::{Decoder, Failure};

pub(crate) fn argument(
    function: &str,
    position: usize,
    parameter: &str,
    detail: &str,
) -> NativeError {
    NativeError::value_error(format!(
        "{function}(): Argument #{position} (${parameter}) {detail}"
    ))
}

pub(crate) const ENCODINGS: &str =
    "must be one of ZLIB_ENCODING_RAW, ZLIB_ENCODING_GZIP, or ZLIB_ENCODING_DEFLATE";

pub(crate) fn encode(
    context: &mut dyn Context,
    values: &[Value],
    function: &'static str,
    default: Option<Encoding>,
) -> NativeResult {
    let (level_arg, encoding_arg) = if default.is_some() { (2, 3) } else { (3, 2) };
    let level = args::int(values, level_arg - 1).unwrap_or(-1);
    if !(-1..=9).contains(&level) {
        return Err(argument(
            function,
            level_arg,
            "level",
            "must be between -1 and 9",
        ));
    }
    let encoding = if let Some(value) = args::int(values, encoding_arg - 1) {
        Encoding::from_php(value)
    } else {
        default
    };
    let encoding =
        encoding.ok_or_else(|| argument(function, encoding_arg, "encoding", ENCODINGS))?;
    let level = if level == -1 { 6 } else { level as u32 };
    let mut encoder = Encoder::new(encoding, level, 15, None);
    match encoder.add(args::bytes(values, 0), Flush::Finish) {
        Ok(bytes) => Ok(Value::from(bytes)),
        Err(()) => {
            context.warning(format!("{function}(): stream error"));
            Ok(Value::Bool(false))
        }
    }
}

pub(crate) fn decode(
    context: &mut dyn Context,
    values: &[Value],
    function: &'static str,
    encoding: Option<Encoding>,
) -> NativeResult {
    let limit = args::int(values, 1).unwrap_or(0);
    if limit < 0 {
        return Err(argument(
            function,
            2,
            "max_length",
            "must be greater than or equal to 0",
        ));
    }
    let bytes = args::bytes(values, 0);
    let result = match encoding {
        _ if bytes.is_empty() => Err(Failure::Data),
        Some(encoding) => bounded(bytes, encoding, limit as usize),
        None => {
            let encoding = if bytes.starts_with(&[0x1f, 0x8b]) {
                Encoding::Gzip
            } else {
                Encoding::Zlib
            };
            bounded(bytes, encoding, limit as usize).or_else(|failure| {
                if failure == Failure::Data {
                    bounded(bytes, Encoding::Raw, limit as usize)
                } else {
                    Err(failure)
                }
            })
        }
    };
    match result {
        Ok(bytes) => Ok(Value::from(bytes)),
        Err(failure) => {
            context.warning(format!("{function}(): {}", failure.message()));
            Ok(Value::Bool(false))
        }
    }
}

// PHP grows the initial buffer by one eighth per round. A final round may
// complete just beyond max_length, but no new round starts once it is reached.
fn bounded(input: &[u8], encoding: Encoding, limit: usize) -> Result<Vec<u8>, Failure> {
    let mut decoder = Decoder::new(encoding, 15, None);
    let limit = if limit == 0 { usize::MAX } else { limit };
    let mut capacity = limit.min(input.len().saturating_add(1));
    let mut offset = 0;
    let mut reached_input_end = false;
    let mut used_terminator = false;
    let mut output = Vec::new();
    for _ in 0..100 {
        if output.len() >= limit {
            return Err(Failure::Memory);
        }
        let mut chunk = [0; 8192];
        loop {
            let bytes = (capacity - output.len()).min(chunk.len());
            // PHP passes its string's implicit NUL after the compressed bytes.
            // Drain the backend's pending output first, so it does not consume
            // that byte ahead of the current PHP-sized output round.
            let at_end = reached_input_end;
            let remaining = match (at_end, used_terminator) {
                (false, _) => &input[offset..],
                (true, false) => &[0],
                (true, true) => &[],
            };
            let (used, wrote) = decoder.feed(remaining, &mut chunk[..bytes])?;
            if at_end {
                used_terminator |= used != 0;
            } else {
                offset += used;
                reached_input_end = offset == input.len() && wrote < bytes;
            }
            output.try_reserve(wrote).map_err(|_| Failure::Memory)?;
            output.extend_from_slice(&chunk[..wrote]);
            if decoder.status == 1 {
                return Ok(output);
            }
            if used_terminator && (output.len() == capacity || wrote < bytes) {
                return Err(Failure::Data);
            }
            if output.len() == capacity {
                break;
            }
            if used == 0 && wrote == 0 && (at_end || offset != input.len()) {
                return Err(Failure::Data);
            }
        }
        capacity = capacity
            .checked_add((capacity >> 3) + 1)
            .ok_or(Failure::Memory)?;
    }
    Err(Failure::Data)
}
