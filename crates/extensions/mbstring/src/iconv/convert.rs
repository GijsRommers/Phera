//! Converting through the platform's iconv with PHP's results:
//!
//! ```php
//! iconv('UTF-8', 'ISO-8859-1//IGNORE', 'aΣb');  // "ab": what the target cannot hold is left out
//! iconv('UTF-8', 'ASCII', "a\xFF");             // false, Notice: Detected an illegal character
//! iconv('UTF-8', 'ASCII', "a\xE2\x82");         // false, Notice: Detected an incomplete multibyte character
//! ```
//!
//! `iconv-native` converts a whole string per call and hides the input
//! position, which shapes two things here.
//!
//! `//IGNORE` is not left to glibc. Its `iconv()` stops with
//! `EILSEQ` after the first internal buffer in which it skipped something,
//! and its caller is expected to call again; PHP does, iconv-native's lossy
//! mode does not, so Phera got only the first few kilobytes of a long
//! string. Phera decodes the input to code points strictly, converts them
//! strictly, and when the target refuses some, finds which ones, leaves
//! them out and converts again; each distinct code point is tried once.
//! Invalid input is left out by decoding the Unicode encodings in Rust and
//! stepping over it in the others. All of that is linear in the input, or
//! O(n log n) for many invalid sequences in a non-Unicode encoding. Apple's
//! iconv does not stop early, so there the platform does the work.
//!
//! An incomplete character at the end is told from the bytes, for the
//! encodings whose structure Phera knows (Unicode, the CJK encodings),
//! rather than from `errno` after the call, which the buffer handling in
//! between could change. For other encodings Phera's notice says "illegal
//! character" where PHP's may say "incomplete multibyte character".

use std::collections::HashMap;

use iconv_native::ConvertError;
pub(super) use phera_extension_api::IconvFailure as Failure;
use phera_extension_api::{Context, NativeError};

use crate::codec::{self, BAD};
use crate::encoding::{ByteOrder, Encoding, Family, read_u32};

/// The code points iconv works through, which every platform converts to
/// and from.
const PIVOT: &str = "UCS-4LE";

/// Converts the way PHP does, without reporting: the work that may run on
/// another thread.
pub fn try_convert(bytes: &[u8], from: &str, to: &str) -> Result<Vec<u8>, Failure> {
    match without_ignore(to) {
        Some(target) if cfg!(target_env = "gnu") => convert_ignoring(bytes, from, &target),
        Some(_) => convert_with_platform_ignore(bytes, from, to),
        None => convert_strictly(bytes, from, to),
    }
}

/// Apple's iconv converts all of the input under `//IGNORE`, and fails only
/// at an incomplete character at the end. What it does with invalid input
/// depends on the target (`?` for each byte into ASCII, nothing into
/// UTF-8), and PHP shows exactly that, so it stays the platform's work.
fn convert_with_platform_ignore(bytes: &[u8], from: &str, to: &str) -> Result<Vec<u8>, Failure> {
    let output =
        iconv_native::convert_lossy(bytes, from, to).map_err(|_| Failure::UnknownConversion)?;
    if incomplete_tail(bytes, from).is_some() {
        return Err(Failure::IncompleteCharacter);
    }
    Ok(output)
}

/// PHP's warning for an unknown encoding, its notice for bad input.
pub fn report(ctx: &mut dyn Context, failure: Failure, from: &str, to: &str, function: &str) {
    match failure {
        Failure::UnknownConversion => ctx.warning(format!(
            "{function}(): Wrong encoding, conversion from \"{from}\" to \"{to}\" is not allowed"
        )),
        Failure::IncompleteCharacter => ctx.notice(format!(
            "{function}(): Detected an incomplete multibyte character in input string"
        )),
        Failure::IllegalCharacter => ctx.notice(format!(
            "{function}(): Detected an illegal character in input string"
        )),
    }
}

/// Converts with PHP's diagnostics, and `None` after one.
pub fn convert(
    ctx: &mut dyn Context,
    bytes: &[u8],
    from: &str,
    to: &str,
    function: &str,
) -> Option<Vec<u8>> {
    try_convert(bytes, from, to)
        .map_err(|failure| report(ctx, failure, from, to, function))
        .ok()
}

/// From this size on, a conversion runs on a blocking thread: copying the
/// input and handing it over costs microseconds, converting it about a
/// millisecond or more.
const OFF_THREAD_BYTES: usize = 64 * 1024;

/// [`convert`] for a string that may be large.
pub async fn convert_text(
    ctx: &mut dyn Context,
    bytes: &[u8],
    from: &str,
    to: &str,
    function: &str,
) -> Result<Option<Vec<u8>>, NativeError> {
    if bytes.len() < OFF_THREAD_BYTES {
        return Ok(convert(ctx, bytes, from, to, function));
    }
    // PHP strings cannot leave the thread, so the work takes copies.
    let (input, source, target) = (bytes.to_vec(), from.to_owned(), to.to_owned());
    let result = tokio::task::spawn_blocking(move || try_convert(&input, &source, &target))
        .await
        .map_err(|_| NativeError::error(format!("{function}(): The conversion did not finish")))?;
    Ok(result
        .map_err(|failure| report(ctx, failure, from, to, function))
        .ok())
}

/// The code points of `bytes`, through UCS-4LE.
pub fn code_points(
    ctx: &mut dyn Context,
    bytes: &[u8],
    encoding: &str,
    function: &str,
) -> Option<Vec<u32>> {
    let ucs4 = convert(ctx, bytes, encoding, PIVOT, function)?;
    Some(points_of_ucs4(&ucs4))
}

pub fn points_of_ucs4(ucs4: &[u8]) -> Vec<u32> {
    ucs4.as_chunks::<4>()
        .0
        .iter()
        .map(|point| read_u32(point))
        .collect()
}

fn ucs4_of_points(points: &[u32]) -> Vec<u8> {
    points
        .iter()
        .flat_map(|point| point.to_le_bytes())
        .collect()
}

/// The target without `//IGNORE`, if it had one: `ASCII//TRANSLIT//IGNORE`
/// becomes `ASCII//TRANSLIT`.
fn without_ignore(to: &str) -> Option<String> {
    let mut parts = to.split("//");
    let name = parts.next().unwrap_or_default();
    let (ignored, kept): (Vec<&str>, Vec<&str>) =
        parts.partition(|part| part.eq_ignore_ascii_case("IGNORE"));
    if ignored.is_empty() {
        return None;
    }
    Some(
        std::iter::once(name)
            .chain(kept)
            .collect::<Vec<_>>()
            .join("//"),
    )
}

fn failure_of(error: ConvertError, bytes: &[u8], from: &str, to: &str) -> Failure {
    match error {
        ConvertError::UnknownConversion => Failure::UnknownConversion,
        ConvertError::InvalidInput if only_incomplete_at_end(bytes, from, to) => {
            Failure::IncompleteCharacter
        }
        ConvertError::InvalidInput => Failure::IllegalCharacter,
    }
}

fn convert_strictly(bytes: &[u8], from: &str, to: &str) -> Result<Vec<u8>, Failure> {
    iconv_native::convert(bytes, from, to).map_err(|error| failure_of(error, bytes, from, to))
}

/// iconv reports `EINVAL`, which PHP calls an incomplete multibyte
/// character, when the input ends inside a character and nothing before
/// that failed: then everything before the incomplete character converts.
fn only_incomplete_at_end(bytes: &[u8], from: &str, to: &str) -> bool {
    incomplete_tail(bytes, from)
        .is_some_and(|start| iconv_native::convert(&bytes[..start], from, to).is_ok())
}

/// Where the last character starts, if the input ends before it does.
fn incomplete_tail(bytes: &[u8], from: &str) -> Option<usize> {
    let encoding = Encoding::find(from.split("//").next().unwrap_or_default())?;
    let length = bytes.len();
    match encoding.family() {
        // The last chunk's invalid bytes, if they are the start of a character.
        Family::Utf8 => {
            let invalid = bytes.utf8_chunks().last()?.invalid();
            let unfinished =
                std::str::from_utf8(invalid).is_err_and(|error| error.error_len().is_none());
            (!invalid.is_empty() && unfinished).then(|| length - invalid.len())
        }
        Family::Ucs4(_) => (!length.is_multiple_of(4)).then(|| length - length % 4),
        Family::Ucs2(_) => (!length.is_multiple_of(2)).then(|| length - 1),
        Family::Utf16(_) if !length.is_multiple_of(2) => Some(length - 1),
        Family::Utf16(order) => {
            let [.., high, low] = bytes else {
                return None;
            };
            let little = order == ByteOrder::Little
                || (order == ByteOrder::Marked && bytes.starts_with(&[0xFF, 0xFE]));
            let unit = if little {
                u16::from_le_bytes([*high, *low])
            } else {
                u16::from_be_bytes([*high, *low])
            };
            (0xD800..=0xDBFF).contains(&unit).then(|| length - 2)
        }
        Family::DoubleByte(_) | Family::Gb18030 => {
            let offsets = codec::boundaries(bytes, encoding);
            let [.., start, end] = offsets[..] else {
                return None;
            };
            (end > length).then_some(start)
        }
        _ => None,
    }
}

/// `//IGNORE` on glibc: the input converted with what the target cannot
/// hold, and invalid input, left out. An input that ends inside a character
/// fails unless the target refused something: glibc then reports `EILSEQ`
/// instead of `EINVAL`, and PHP returns what was converted. Skipped invalid
/// input does not change the report.
fn convert_ignoring(bytes: &[u8], from: &str, target: &str) -> Result<Vec<u8>, Failure> {
    let source = decode_dropping(bytes, from)?;
    let (output, refused_any) = encode_dropping(&source.points, target)?;
    if source.incomplete_at_end && !refused_any {
        return Err(Failure::IncompleteCharacter);
    }
    Ok(output)
}

/// The input as code points, with invalid sequences left out.
struct Source {
    points: Vec<u32>,
    /// The input ended inside a character, which was left out too.
    incomplete_at_end: bool,
}

fn decode_dropping(bytes: &[u8], from: &str) -> Result<Source, Failure> {
    match iconv_native::convert(bytes, from, PIVOT) {
        Ok(ucs4) => {
            return Ok(Source {
                points: points_of_ucs4(&ucs4),
                incomplete_at_end: false,
            });
        }
        Err(ConvertError::UnknownConversion) => return Err(Failure::UnknownConversion),
        Err(ConvertError::InvalidInput) => {}
    }
    let tail = incomplete_tail(bytes, from);
    let unicode = Encoding::find(from.split("//").next().unwrap_or_default()).filter(|encoding| {
        matches!(
            encoding.family(),
            Family::Utf8 | Family::Utf16(_) | Family::Ucs2(_) | Family::Ucs4(_)
        )
    });
    match unicode {
        Some(encoding) => Ok(decode_unicode_dropping(bytes, encoding, tail)),
        None => decode_stepping(bytes, from, tail),
    }
}

/// The Unicode encodings are decoded here, with each invalid unit left out
/// as iconv's `//IGNORE` does.
fn decode_unicode_dropping(bytes: &[u8], encoding: Encoding, tail: Option<usize>) -> Source {
    let complete = &bytes[..tail.unwrap_or(bytes.len())];
    let decoded = codec::decode(complete, encoding);
    let points: Vec<u32> = decoded
        .iter()
        .copied()
        .filter(|&point| point != BAD)
        .collect();
    Source {
        points,
        incomplete_at_end: tail.is_some(),
    }
}

/// Other encodings go through iconv piece by piece: up to the next invalid
/// sequence, then one byte past it, as PHP steps. Finding each invalid
/// sequence costs conversions of about the length since the last one, so
/// the whole is O(n log n).
fn decode_stepping(bytes: &[u8], from: &str, tail: Option<usize>) -> Result<Source, Failure> {
    let mut source = Source {
        points: Vec::with_capacity(bytes.len()),
        incomplete_at_end: false,
    };
    let mut position = 0;
    while position < bytes.len() {
        let rest = &bytes[position..];
        let (points, valid) = valid_prefix(rest, from)?;
        source.points.extend(points);
        position += valid;
        if position >= bytes.len() {
            break;
        }
        if Some(position) == tail {
            source.incomplete_at_end = true;
            break;
        }
        position += 1;
    }
    Ok(source)
}

/// The code points before the first invalid sequence of `bytes`, and how
/// many bytes they take.
fn valid_prefix(bytes: &[u8], from: &str) -> Result<(Vec<u32>, usize), Failure> {
    let converted = |length: usize| -> Result<Vec<u32>, Failure> {
        // A target ending in `//` keeps iconv-native's lossy mode from adding
        // //IGNORE//TRANSLIT: it converts up to the first error.
        iconv_native::convert_lossy(&bytes[..length], from, "UCS-4LE//")
            .map(|ucs4| points_of_ucs4(&ucs4))
            .map_err(|_| Failure::UnknownConversion)
    };
    let points = converted(bytes.len())?;
    let wanted = points.len();
    // The shortest prefix that gives all of them ends where the error
    // starts: double a guess until it does, then halve the difference.
    let mut low = 0;
    let mut high = wanted.max(1).min(bytes.len());
    while converted(high)?.len() < wanted {
        low = high;
        high = (high * 2).min(bytes.len());
    }
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        if converted(middle)?.len() < wanted {
            low = middle;
        } else {
            high = middle;
        }
    }
    let valid = if wanted == 0 { 0 } else { high };
    Ok((points, valid))
}

/// The code points converted to the target with those it refuses left out,
/// and whether there were any.
fn encode_dropping(points: &[u32], target: &str) -> Result<(Vec<u8>, bool), Failure> {
    match iconv_native::convert(ucs4_of_points(points), PIVOT, target) {
        Ok(output) => return Ok((output, false)),
        Err(ConvertError::UnknownConversion) => return Err(Failure::UnknownConversion),
        Err(ConvertError::InvalidInput) => {}
    }
    let accepted = accepted_points(points, target);
    let kept: Vec<u32> = points
        .iter()
        .copied()
        .filter(|point| accepted.get(point).copied().unwrap_or(false))
        .collect();
    // Each kept code point converts on its own; together they did not
    // only if the target's state between characters refuses something,
    // which no platform encoding does.
    let output = iconv_native::convert(ucs4_of_points(&kept), PIVOT, target)
        .map_err(|_| Failure::IllegalCharacter)?;
    Ok((output, true))
}

/// Distinct code points are tried this many at a time: one conversion
/// clears a group the target fully accepts, and only a group with a refused
/// one is tried point by point.
const GROUP: usize = 64;

/// Which of the code points the target accepts, each found out once, so
/// that a repeated character costs a lookup.
fn accepted_points(points: &[u32], target: &str) -> HashMap<u32, bool> {
    let mut accepted = HashMap::new();
    let mut distinct = Vec::new();
    for &point in points {
        if let std::collections::hash_map::Entry::Vacant(entry) = accepted.entry(point) {
            entry.insert(false);
            distinct.push(point);
        }
    }
    let converts =
        |group: &[u32]| iconv_native::convert(ucs4_of_points(group), PIVOT, target).is_ok();
    for group in distinct.chunks(GROUP) {
        if converts(group) {
            for point in group {
                accepted.insert(*point, true);
            }
            continue;
        }
        for &point in group {
            accepted.insert(point, converts(&[point]));
        }
    }
    accepted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_is_taken_out_of_the_target() {
        assert_eq!(
            without_ignore("ASCII//TRANSLIT//IGNORE").as_deref(),
            Some("ASCII//TRANSLIT")
        );
        assert_eq!(without_ignore("ascii//ignore").as_deref(), Some("ascii"));
        assert_eq!(without_ignore("ASCII//TRANSLIT"), None);
    }

    #[test]
    fn an_incomplete_last_character_is_found_from_the_bytes() {
        assert_eq!(incomplete_tail(b"a\xE2\x82", "UTF-8"), Some(1));
        assert_eq!(incomplete_tail(b"\xFFa\xE2\x82", "UTF-8"), Some(2));
        assert_eq!(incomplete_tail(b"a\xFF", "UTF-8"), None);
        assert_eq!(incomplete_tail(b"\x00a\x00", "UTF-16BE"), Some(2));
        assert_eq!(incomplete_tail(b"\x00a\xD8\x00", "UTF-16BE"), Some(2));
        assert_eq!(incomplete_tail(b"ab\xB0", "EUC-KR"), Some(2));
        assert_eq!(incomplete_tail(b"abc", "ISO-8859-1"), None);
        assert_eq!(incomplete_tail(b"abc", "no such encoding"), None);
    }

    /// The glibc path, which compiles everywhere: a long string loses only
    /// what the target refuses, as many times as it occurs.
    #[test]
    fn a_long_string_loses_only_what_the_target_refuses() {
        let text = "AéΣ中😀".repeat(50_000);
        let expected: Vec<u8> = text
            .chars()
            .filter_map(|character| u8::try_from(u32::from(character)).ok())
            .collect();
        assert_eq!(
            convert_ignoring(text.as_bytes(), "UTF-8", "ISO-8859-1"),
            Ok(expected)
        );
    }

    #[test]
    fn invalid_input_is_stepped_over_and_an_incomplete_end_noticed() {
        let source = decode_dropping(b"ab\xFFcd\xB0", "EUC-KR").map_err(|_| ());
        let source = source.expect("EUC-KR is known");
        assert_eq!(source.points, [0x61, 0x62, 0x63, 0x64]);
        assert!(source.incomplete_at_end);
    }
}
