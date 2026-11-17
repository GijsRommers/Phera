//! Character offsets differ from byte offsets: `mb_substr('aéz', 1, 1) === 'é'`.
use crate::{
    decode::decode,
    encode::encode,
    encoding,
    state::Settings,
    unicode::{self, Case},
};
use phera_extension_api::{Context, NativeError, NativeResult, PhpArray, Value, args};

pub fn points(
    ctx: &mut dyn Context,
    a: &[Value],
    index: usize,
    name: &str,
) -> Result<(Vec<u32>, encoding::Encoding), NativeError> {
    let enc = encoding::argument(ctx, a, index, name)?;
    Ok((decode(args::bytes(a, 0), enc), enc))
}
pub fn result(ctx: &mut dyn Context, points: &[u32], enc: encoding::Encoding) -> NativeResult {
    Ok(Value::from(encode(
        points,
        enc,
        &ctx.state().get_or_default::<Settings>().substitute,
    )))
}
pub fn strlen(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let enc = encoding::argument(ctx, a, 1, "mb_strlen")?;
    let bytes = args::bytes(a, 0);
    let count = if enc.0 == 45 {
        bytes.len()
    } else if matches!(enc.0, 2..=4 | 8..=10) {
        bytes.len() / 4
    } else if matches!(enc.0, 5..=7) {
        bytes.len() / 2
    } else if enc == encoding::Encoding::UTF8 {
        std::str::from_utf8(bytes).map_or_else(|_| decode(bytes, enc).len(), |s| s.chars().count())
    } else {
        decode(bytes, enc).len()
    };
    Ok(Value::Int(count as i64))
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
    let bytes = args::bytes(a, 0);
    if e == encoding::Encoding::UTF8
        && let Ok(valid) = std::str::from_utf8(bytes)
    {
        let (start, end) = bounds(
            valid.chars().count(),
            args::int(a, 1).unwrap_or(0),
            args::int(a, 2),
        );
        let mut indices = valid.char_indices().map(|(i, _)| i).chain([bytes.len()]);
        let first = indices.nth(start).unwrap_or(bytes.len());
        let last = if end == start {
            first
        } else {
            indices.nth(end - start - 1).unwrap_or(bytes.len())
        };
        return Ok(Value::from(&bytes[first..last]));
    }
    let p = decode(bytes, e);
    if crate::boundary::fixed(e) {
        let bytes = args::bytes(a, 0);
        let size = match e.0 {
            2..=4 | 8..=10 => 4,
            5..=7 => 2,
            _ => 1,
        };
        let characters = (bytes.len() / size) as i64;
        let requested = args::int(a, 1).unwrap_or(0);
        let start = if requested < 0 {
            characters.saturating_add(requested).max(0)
        } else {
            requested
        } as usize;
        let first = start.saturating_mul(size).min(bytes.len());
        let length = match args::int(a, 2) {
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
        return Ok(Value::from(&bytes[first..end]));
    }
    let (start, end) = bounds(p.len(), args::int(a, 1).unwrap_or(0), args::int(a, 2));
    result(ctx, &p[start..end], e)
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
    if !matches!(e.0, 11..=13 | 20 | 21) {
        let bytes = args::bytes(a, 0);
        let offsets = crate::boundary::offsets(bytes, e);
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
    if e.0 == 45 && p.is_empty() && !args::bytes(a, 0).is_empty() {
        return Ok(Value::from("begin 0644 filename\n "));
    }
    if first && !p.is_empty() {
        let first_args = vec![
            args::value(a, 0).clone(),
            Value::Int(0),
            Value::Int(1),
            Value::from(e.name()),
        ];
        let initial = substr(ctx, &first_args)?;
        let head = decode(args::bytes(std::slice::from_ref(&initial), 0), e);
        let mapped = result(ctx, &unicode::convert(&head, case), e)?;
        if initial == mapped {
            return Ok(args::value(a, 0).clone());
        }
        let rest = substr(
            ctx,
            &[
                args::value(a, 0).clone(),
                Value::Int(1),
                Value::Null,
                Value::from(e.name()),
            ],
        )?;
        let mut bytes = args::bytes(&[mapped], 0).to_vec();
        bytes.extend_from_slice(args::bytes(&[rest], 0));
        return Ok(Value::from(bytes));
    }
    result(ctx, &unicode::convert(&p, case), e)
}

pub fn convert_case(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let mode = Case::parse(args::int(a, 1).unwrap_or(0)).ok_or_else(|| {
        NativeError::value_error(
            "mb_convert_case(): Argument #2 ($mode) must be one of the MB_CASE_* constants",
        )
    })?;
    let (p, e) = points(ctx, a, 2, "mb_convert_case")?;
    result(ctx, &unicode::convert(&p, mode), e)
}
pub fn ord(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (p, e) = points(ctx, a, 1, "mb_ord")?;
    if e.0 == 20 || e.0 >= 44 {
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
        .filter(|cp| **cp != u32::MAX)
        .map_or(Value::Bool(false), |cp| Value::Int(i64::from(*cp))))
}
pub fn chr(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let e = encoding::argument(ctx, a, 1, "mb_chr")?;
    if e.0 == 20 || e.0 >= 44 {
        return Err(NativeError::value_error(format!(
            "mb_chr() does not support the \"{}\" encoding",
            e.name()
        )));
    }
    let cp = args::int(a, 0).unwrap_or(0);
    if (0xd800..=0xdfff).contains(&cp) && matches!(e.0, 2..=13) {
        let bytes = if matches!(e.0, 2..=4 | 8..=10) {
            if matches!(e.0, 4 | 10) {
                (cp as u32).to_le_bytes()
            } else {
                (cp as u32).to_be_bytes()
            }
            .to_vec()
        } else {
            if matches!(e.0, 7 | 13) {
                (cp as u16).to_le_bytes()
            } else {
                (cp as u16).to_be_bytes()
            }
            .to_vec()
        };
        return Ok(Value::from(bytes));
    }

    if !(0..=0x10ffff).contains(&cp) || (0xd800..=0xdfff).contains(&cp) {
        return Ok(Value::Bool(false));
    }
    let bytes = encode(&[cp as u32], e, &crate::encode::Substitute::None);
    Ok(if bytes.is_empty() {
        Value::Bool(false)
    } else {
        Value::from(bytes)
    })
}
pub fn scrub(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (p, e) = points(ctx, a, 1, "mb_scrub")?;
    if matches!(e.0, 44 | 47) {
        let raw: Vec<_> = args::bytes(a, 0).iter().map(|b| u32::from(*b)).collect();
        return result(ctx, &raw, e);
    }
    if e.0 == 45 {
        return Ok(Value::from(
            p.iter().map(|cp| *cp as u8).collect::<Vec<_>>(),
        ));
    }
    result(ctx, &p, e)
}
