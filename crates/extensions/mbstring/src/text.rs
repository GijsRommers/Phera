//! Character offsets differ from byte offsets: `mb_substr('aéz', 1, 1) === 'é'`.
use crate::{
    codec::{BAD, Substitute, decode, encode},
    encoding::{self, Encoding, Family, Transfer},
    state::Settings,
    unicode::{self, Case},
};
use std::borrow::Cow;

use phera_extension_api::{Context, NativeError, NativeResult, PhpArray, Value, args};

pub fn points(
    ctx: &mut dyn Context,
    a: &[Value],
    index: usize,
    name: &str,
) -> Result<(Vec<u32>, Encoding), NativeError> {
    let enc = encoding::argument(ctx, a, index, name)?;
    Ok((decode(args::bytes(a, 0), enc), enc))
}
pub fn result(ctx: &mut dyn Context, points: &[u32], enc: Encoding) -> NativeResult {
    Ok(Value::from(encode(
        points,
        enc,
        &ctx.state().get_or_default::<Settings>().substitute,
    )))
}
pub fn strlen(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let enc = encoding::argument(ctx, a, 1, "mb_strlen")?;
    Ok(Value::Int(character_count(args::bytes(a, 0), enc) as i64))
}
/// `mb_strlen` for an encoding already resolved.
pub fn character_count(bytes: &[u8], enc: Encoding) -> usize {
    if enc == Encoding::Uuencode {
        bytes.len()
    } else if let Some(width) = enc.fixed_width() {
        bytes.len() / width
    } else if enc == Encoding::Utf8 {
        std::str::from_utf8(bytes).map_or_else(|_| decode(bytes, enc).len(), |s| s.chars().count())
    } else {
        decode(bytes, enc).len()
    }
}
pub fn bounds(size: usize, start: i64, length: Option<i64>) -> (usize, usize) {
    let size = size as i64;
    let start = if start < 0 {
        size.saturating_add(start).max(0)
    } else {
        start.min(size)
    };
    let end = match length {
        None => size,
        Some(n) if n < 0 => size.saturating_add(n).max(start),
        Some(n) => start.saturating_add(n).min(size),
    };
    (start as usize, end as usize)
}
pub fn substr(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    for (index, parameter) in [(1, "start"), (2, "length")] {
        if args::int(a, index) == Some(i64::MIN) {
            return Err(NativeError::value_error(format!(
                "mb_substr(): Argument #{} (${parameter}) must be between -{} and {}",
                index + 1,
                i64::MAX,
                i64::MAX
            )));
        }
    }
    let e = encoding::argument(ctx, a, 3, "mb_substr")?;
    let start = args::int(a, 1).unwrap_or(0);
    let substring = substring(ctx, args::bytes(a, 0), start, args::int(a, 2), e);
    Ok(Value::from(substring.into_owned()))
}
/// `mb_substr` for an encoding already resolved. Valid UTF-8 and the
/// fixed-width encodings are sliced without decoding, so the result can
/// borrow from `bytes`.
pub fn substring<'a>(
    ctx: &mut dyn Context,
    bytes: &'a [u8],
    start: i64,
    length: Option<i64>,
    e: Encoding,
) -> Cow<'a, [u8]> {
    if e == Encoding::Utf8
        && let Ok(valid) = std::str::from_utf8(bytes)
    {
        let (start, end) = bounds(valid.chars().count(), start, length);
        let mut indices = valid.char_indices().map(|(i, _)| i).chain([bytes.len()]);
        let first = indices.nth(start).unwrap_or(bytes.len());
        let last = if end == start {
            first
        } else {
            indices.nth(end - start - 1).unwrap_or(bytes.len())
        };
        return Cow::Borrowed(&bytes[first..last]);
    }
    if let Some(size) = e.fixed_width() {
        let characters = (bytes.len() / size) as i64;
        let requested = start;
        let start = if requested < 0 {
            characters.saturating_add(requested).max(0)
        } else {
            requested
        } as usize;
        let first = start.saturating_mul(size).min(bytes.len());
        let length = match length {
            None => bytes.len(),
            Some(n) if n >= 0 => (n as usize).saturating_mul(size),
            Some(n) => {
                characters
                    .saturating_sub(start as i64)
                    .saturating_add(n)
                    .max(0) as usize
                    * size
            }
        };
        let end = first.saturating_add(length).min(bytes.len());
        return Cow::Borrowed(&bytes[first..end]);
    }
    let p = decode(bytes, e);
    let (start, end) = bounds(p.len(), start, length);
    let substitute = &ctx.state().get_or_default::<Settings>().substitute;
    Cow::Owned(encode(&p[start..end], e, substitute))
}
pub fn split(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let n = args::int(a, 1).unwrap_or(1);
    if n <= 0 {
        return Err(NativeError::value_error(
            "mb_str_split(): Argument #2 ($length) must be greater than 0",
        ));
    }
    let (p, e) = points(ctx, a, 2, "mb_str_split")?;
    let mut out = PhpArray::new();
    if !matches!(
        e.family(),
        Family::Utf16(_) | Family::Iso2022Jp | Family::Gb18030
    ) {
        let bytes = args::bytes(a, 0);
        let offsets = crate::codec::boundaries(bytes, e);
        let mut i = 0;
        while i + 1 < offsets.len() {
            let next = i.saturating_add(n as usize).min(offsets.len() - 1);
            out.push(Value::from(
                &bytes[offsets[i]..offsets[next].min(bytes.len())],
            ))?;
            i = next;
        }
    } else {
        for chunk in p.chunks(n as usize) {
            out.push(result(ctx, chunk, e)?)?;
        }
    }
    Ok(Value::from(out))
}
pub fn width(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (p, _) = points(ctx, a, 1, "mb_strwidth")?;
    Ok(Value::Int(
        p.into_iter().map(unicode::width).sum::<usize>() as i64
    ))
}
pub fn upper(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    casing(ctx, a, "mb_strtoupper", Case::Upper, false)
}
pub fn lower(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    casing(ctx, a, "mb_strtolower", Case::Lower, false)
}
pub fn ucfirst(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    casing(ctx, a, "mb_ucfirst", Case::Title, true)
}
pub fn lcfirst(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    casing(ctx, a, "mb_lcfirst", Case::Lower, true)
}
fn casing(ctx: &mut dyn Context, a: &[Value], name: &str, case: Case, first: bool) -> NativeResult {
    let (p, e) = points(ctx, a, 1, name)?;
    if e == Encoding::Uuencode && p.is_empty() && !args::bytes(a, 0).is_empty() {
        return Ok(Value::from("begin 0644 filename\n "));
    }
    if first && !p.is_empty() {
        let bytes = args::bytes(a, 0);
        let initial = substring(ctx, bytes, 0, Some(1), e);
        let head = decode(&initial, e);
        let mapped = result(ctx, &unicode::convert(&head, case), e)?;
        if args::bytes(std::slice::from_ref(&mapped), 0) == &initial[..] {
            return Ok(args::value(a, 0).clone());
        }
        let rest = substring(ctx, bytes, 1, None, e);
        let mut bytes = args::bytes(&[mapped], 0).to_vec();
        bytes.extend_from_slice(&rest);
        return Ok(Value::from(bytes));
    }
    result(ctx, &unicode::convert(&p, case), e)
}

pub fn convert_case(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let mode = Case::from_mode(args::int(a, 1).unwrap_or(0)).ok_or_else(|| {
        NativeError::value_error(
            "mb_convert_case(): Argument #2 ($mode) must be one of the MB_CASE_* constants",
        )
    })?;
    let (p, e) = points(ctx, a, 2, "mb_convert_case")?;
    result(ctx, &unicode::convert(&p, mode), e)
}
pub fn ord(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (p, e) = points(ctx, a, 1, "mb_ord")?;
    if e == Encoding::Iso2022Jp || e.transfer().is_some() {
        return Err(NativeError::value_error(format!(
            "mb_ord() does not support the \"{}\" encoding",
            e.name()
        )));
    }
    if p.is_empty() {
        return Err(NativeError::value_error(
            "mb_ord(): Argument #1 ($string) must not be empty",
        ));
    }
    Ok(p.first()
        .filter(|cp| **cp != BAD)
        .map_or(Value::Bool(false), |cp| Value::Int(i64::from(*cp))))
}
pub fn chr(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let e = encoding::argument(ctx, a, 1, "mb_chr")?;
    if e == Encoding::Iso2022Jp || e.transfer().is_some() {
        return Err(NativeError::value_error(format!(
            "mb_chr() does not support the \"{}\" encoding",
            e.name()
        )));
    }
    let cp = args::int(a, 0).unwrap_or(0);
    // Lone surrogates are not characters, but the UTF-16, UTF-32, UCS-2 and
    // UCS-4 encoders write them as they are, as PHP does; every other
    // encoder rejects them.
    if !(0..=0x10ffff).contains(&cp) {
        return Ok(Value::Bool(false));
    }
    let bytes = encode(&[cp as u32], e, &Substitute::None);
    Ok(if bytes.is_empty() {
        Value::Bool(false)
    } else {
        Value::from(bytes)
    })
}
pub fn scrub(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (p, e) = points(ctx, a, 1, "mb_scrub")?;
    if matches!(
        e.transfer(),
        Some(Transfer::Base64 | Transfer::QuotedPrintable)
    ) {
        let raw: Vec<_> = args::bytes(a, 0).iter().map(|b| u32::from(*b)).collect();
        return result(ctx, &raw, e);
    }
    if e == Encoding::Uuencode {
        return Ok(Value::from(
            p.iter().map(|cp| *cp as u8).collect::<Vec<_>>(),
        ));
    }
    result(ctx, &p, e)
}
