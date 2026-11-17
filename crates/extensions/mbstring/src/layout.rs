//! Trimming and padding count Unicode characters: `mb_str_pad('é', 3) === 'é  '`.
use crate::{
    codec::{Substitute, boundaries, decode, encode},
    encoding::{Encoding, Family},
    text, unicode,
};
use phera_extension_api::{Context, NativeError, NativeResult, Value, args};
const SPACE: &[u32] = &[
    0, 9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197, 8198, 8199, 8200,
    8201, 8202, 8232, 8233, 8239, 8287, 12288,
];
#[derive(Clone, Copy)]
pub enum Trim {
    Both,
    Left,
    Right,
}
pub fn trim(ctx: &mut dyn Context, a: &[Value], name: &str, side: Trim) -> NativeResult {
    let (p, e) = text::points(ctx, a, 2, name)?;
    let custom = args::string(a, 1).map(|s| decode(s.as_bytes(), e));
    let set = custom.as_deref().unwrap_or(SPACE);
    let start = if matches!(side, Trim::Right) {
        0
    } else {
        p.iter().position(|c| !set.contains(c)).unwrap_or(p.len())
    };
    let end = if matches!(side, Trim::Left) {
        p.len()
    } else {
        p.iter()
            .rposition(|c| !set.contains(c))
            .map_or(start, |i| i + 1)
    };
    if start == 0 && end == p.len() {
        return Ok(args::value(a, 0).clone());
    }
    text::substr(
        ctx,
        &[
            args::value(a, 0).clone(),
            Value::Int(start as i64),
            Value::Int(end.saturating_sub(start) as i64),
            Value::from(e.name()),
        ],
    )
}

pub fn pad(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (p, e) = text::points(ctx, a, 4, "mb_str_pad")?;
    let length = args::int(a, 1).unwrap_or(0);
    if length <= p.len() as i64 {
        return Ok(args::value(a, 0).clone());
    }

    let padding = args::string(a, 2).cloned().unwrap_or_else(|| " ".into());
    let pad_args = [Value::String(padding.clone()), Value::from(e.name())];
    let Value::Int(pad_length) = text::strlen(ctx, &pad_args)? else {
        unreachable!()
    };
    if pad_length == 0 {
        return Err(NativeError::value_error(
            "mb_str_pad(): Argument #3 ($pad_string) must not be empty",
        ));
    }
    let kind = args::int(a, 3).unwrap_or(1);
    if !(0..=2).contains(&kind) {
        return Err(NativeError::value_error(concat!(
            "mb_str_pad(): Argument #4 ($pad_type) ",
            "must be STR_PAD_LEFT, STR_PAD_RIGHT, or STR_PAD_BOTH"
        )));
    }
    let extra = length as usize - p.len();
    let left = match kind {
        0 => extra,
        2 => extra / 2,
        _ => 0,
    };
    let mut output = Vec::new();
    output
        .try_reserve(length as usize)
        .map_err(|_| NativeError::error("Allowed memory size exhausted"))?;
    for (index, count) in [left, extra - left].into_iter().enumerate() {
        if index == 1 {
            output.extend_from_slice(args::bytes(a, 0));
        }
        for _ in 0..count / pad_length as usize {
            output.extend_from_slice(padding.as_bytes());
        }
        let tail = text::substr(
            ctx,
            &[
                Value::String(padding.clone()),
                Value::Int(0),
                Value::Int((count % pad_length as usize) as i64),
                Value::from(e.name()),
            ],
        )?;
        output.extend_from_slice(args::bytes(&[tail], 0));
    }
    Ok(Value::from(output))
}

pub fn cut(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (p, e) = text::points(ctx, a, 3, "mb_strcut")?;
    let bytes = args::bytes(a, 0);
    let (start, end) = text::bounds(bytes.len(), args::int(a, 1).unwrap_or(0), args::int(a, 2));
    if args::int(a, 1).is_some_and(|n| n > bytes.len() as i64) {
        return Ok(Value::from(""));
    }

    let length = match args::int(a, 2) {
        None => bytes.len(),
        Some(n) if n >= 0 => n as usize,
        Some(_) => end.saturating_sub(start),
    };
    if e == Encoding::Utf8 {
        let mut first = start;
        while first > 0 && first < bytes.len() && bytes[first] & 0xc0 == 0x80 {
            first -= 1;
        }
        let mut last = first.saturating_add(length).min(bytes.len());
        while last > first && last < bytes.len() && bytes[last] & 0xc0 == 0x80 {
            last -= 1;
        }
        return Ok(Value::from(&bytes[first..last]));
    }
    if let Family::Utf16(order) = e.family() {
        let first = start & !1;
        let length = length.min(bytes.len() - start) & !1;
        let mut last = first + length;
        if last >= first + 2 {
            let unit = if order == crate::encoding::ByteOrder::Little {
                u16::from_le_bytes([bytes[last - 2], bytes[last - 1]])
            } else {
                u16::from_be_bytes([bytes[last - 2], bytes[last - 1]])
            };
            if (0xd800..=0xdbff).contains(&unit) {
                last -= 2;
            }
        }
        return Ok(Value::from(&bytes[first..last]));
    }
    if e != Encoding::Iso2022Jp {
        let offsets = boundaries(bytes, e);
        let aligned = offsets
            .iter()
            .copied()
            .take_while(|n| *n <= start)
            .last()
            .unwrap_or(0);
        let length = if e == Encoding::Gb18030 {
            length.min(bytes.len() - start)
        } else {
            length
        };
        let limit = aligned.saturating_add(length).min(bytes.len());
        let mut end = offsets
            .iter()
            .copied()
            .take_while(|n| *n <= limit)
            .last()
            .unwrap_or(aligned);
        if limit >= bytes.len() {
            end = bytes.len();
        }
        let size = match e.family() {
            Family::Ucs4(_) => 4,
            Family::Ucs2(_) | Family::Utf16(_) => 2,
            _ => 1,
        };
        end -= end % size;
        return Ok(Value::from(&bytes[aligned.min(end)..end]));
    }
    let mut at = 0;
    let mut selected = Vec::new();
    for cp in p {
        let encoded = encode(&[cp], e, &Substitute::default());
        let next = at + encoded.len();
        if next > start && next <= end {
            selected.push(cp);
        }
        at = next;
    }
    text::result(ctx, &selected, e)
}
pub fn imwidth(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (p, e) = text::points(ctx, a, 4, "mb_strimwidth")?;
    let start = args::int(a, 1).unwrap_or(0);
    let start = if start < 0 {
        (p.len() as i64).saturating_add(start)
    } else {
        start
    };
    if start < 0 || start > p.len() as i64 {
        return Err(NativeError::value_error(
            "mb_strimwidth(): Argument #2 ($start) is out of range",
        ));
    }
    let mut width = args::int(a, 2).unwrap_or(0);
    if width < 0 {
        ctx.deprecated(
            "mb_strimwidth(): passing a negative integer to argument #3 ($width) is deprecated"
                .into(),
        );
        width += p.iter().map(|c| unicode::width(*c) as i64).sum::<i64>();
        width -= p[..start as usize]
            .iter()
            .map(|c| unicode::width(*c) as i64)
            .sum::<i64>();
    }
    if width < 0 {
        return Err(NativeError::value_error(
            "mb_strimwidth(): Argument #3 ($width) is out of range",
        ));
    }
    let rest = &p[start as usize..];
    if rest.iter().map(|c| unicode::width(*c)).sum::<usize>() <= width as usize {
        return text::result(ctx, rest, e);
    }
    let marker = decode(args::bytes(a, 3), e);
    let marker_width: usize = marker.iter().map(|c| unicode::width(*c)).sum();
    let budget = (width as usize).saturating_sub(marker_width);
    let mut used = 0;
    let mut out = Vec::new();
    for cp in rest {
        used += unicode::width(*cp);
        if used > budget {
            break;
        }
        out.push(*cp);
    }
    let value = text::result(ctx, &out, e)?;
    let mut bytes = args::bytes(&[value], 0).to_vec();
    bytes.extend_from_slice(args::bytes(a, 3));
    Ok(Value::from(bytes))
}
