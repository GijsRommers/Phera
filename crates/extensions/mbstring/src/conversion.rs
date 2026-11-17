//! Conversion walks PHP arrays iteratively: `mb_convert_encoding(['é'], 'ASCII')`.
use crate::{
    codec::{self, BAD, decode},
    encoding::{self, Encoding, Transfer},
    settings,
    state::Settings,
    text,
};
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, args};

pub fn encoding_list(
    ctx: &mut dyn Context,
    value: &Value,
    function: &str,
    number: usize,
    parameter: &str,
) -> Result<Vec<Encoding>, NativeError> {
    if value.is_null() {
        return Ok(ctx.state().get_or_default::<Settings>().detect.clone());
    }
    let names: Vec<String> = match value {
        Value::Array(a) => a
            .iter()
            .map(|(_, v)| match v {
                Value::String(s) => s.to_string_lossy(),
                other => crate::encoding_name(other),
            })
            .collect(),
        Value::String(s) => s.to_string_lossy().split(',').map(str::to_owned).collect(),
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for name in names {
        let name = name.trim();
        if name.eq_ignore_ascii_case("auto") {
            out.extend(
                ctx.state()
                    .get_or_default::<Settings>()
                    .detect
                    .iter()
                    .copied(),
            );
        } else if let Some(e) = Encoding::find(name) {
            out.push(e);
        } else if !name.is_empty() {
            return Err(NativeError::value_error(format!(
                "{function}(): Argument #{number} (${parameter}) \
                 contains invalid encoding \"{name}\""
            )));
        }
    }
    if out.is_empty() {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #{number} (${parameter}) must specify at least one encoding"
        )));
    }
    Ok(out)
}
pub fn order(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    if args::value(a, 0).is_null() {
        return Ok(settings::list(
            ctx.state()
                .get_or_default::<Settings>()
                .detect
                .iter()
                .map(|e| Value::from(e.name())),
        ));
    }
    let list = encoding_list(ctx, args::value(a, 0), "mb_detect_order", 1, "encoding")?;
    ctx.state().get_or_default::<Settings>().detect = list;
    Ok(Value::Bool(true))
}
pub fn detect(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let list = encoding_list(ctx, args::value(a, 1), "mb_detect_encoding", 2, "encodings")?;
    Ok(
        guess(args::bytes(a, 0), &list, args::bool(a, 2).unwrap_or(false))
            .map_or(Value::Bool(false), |e| Value::from(e.name())),
    )
}
pub fn guess(bytes: &[u8], encodings: &[Encoding], strict: bool) -> Option<Encoding> {
    encodings
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, encoding)| {
            let p = decode(bytes, encoding);
            if strict && p.contains(&BAD) {
                return None;
            }
            let mut score: usize = p
                .into_iter()
                .map(|cp| match cp {
                    BAD => 1000,
                    0x10000.. => 40,
                    0x21..=0x2f => 6,
                    _ => {
                        let table = include_bytes!("../data/rare.bin");
                        if table[cp as usize / 8] & (1 << (cp % 8)) != 0 {
                            30
                        } else {
                            1
                        }
                    }
                })
                .sum();
            score = (score as f64 * (1.0 + 0.3 * index as f64 / encodings.len() as f64)) as usize;
            Some((encoding, score))
        })
        .min_by_key(|(_, score)| *score)
        .map(|(e, _)| e)
}
pub fn check(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let e = encoding::argument(ctx, a, 1, "mb_check_encoding")?;
    if args::value(a, 0).is_null() {
        ctx.deprecated(
            "mb_check_encoding(): Calling mb_check_encoding() without argument is deprecated"
                .into(),
        );
        return Ok(Value::Bool(true));
    }
    let Ok(strings) = crate::walk::strings(args::value(a, 0).clone(), true) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(
        strings.iter().all(|s| !decode(s, e).contains(&BAD)),
    ))
}

pub fn convert(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let to = encoding::named(
        ctx,
        &String::from_utf8_lossy(args::bytes(a, 1)),
        "mb_convert_encoding",
        2,
        "to_encoding",
    )?;
    let list = if args::value(a, 2).is_null() {
        vec![ctx.state().get_or_default::<Settings>().internal]
    } else {
        encoding_list(
            ctx,
            args::value(a, 2),
            "mb_convert_encoding",
            3,
            "from_encoding",
        )?
    };
    // Base64 and Quoted-Printable encode the bytes, whatever their encoding.
    if matches!(
        to.transfer(),
        Some(Transfer::Base64 | Transfer::QuotedPrintable)
    ) && matches!(args::value(a, 0), Value::String(_))
    {
        let points: Vec<_> = args::bytes(a, 0).iter().map(|b| u32::from(*b)).collect();
        return Ok(Value::from(codec::encode(
            &points,
            to,
            &codec::Substitute::None,
        )));
    }
    // Decoding Base64, uuencode or Quoted-Printable gives bytes, not text.
    if let [from] = list[..]
        && matches!(
            from.transfer(),
            Some(Transfer::Base64 | Transfer::Uuencode | Transfer::QuotedPrintable)
        )
    {
        let points = decode(args::bytes(a, 0), from);
        let substitute = ctx.state().get_or_default::<Settings>().substitute.clone();
        return Ok(Value::from(codec::encode(
            &points,
            Encoding::EightBit,
            &substitute,
        )));
    }
    transform(ctx, args::value(a, 0).clone(), to, &list)
}
enum Work {
    Value(Value),
    Array(Vec<ArrayKey>),
}
pub fn transform(
    ctx: &mut dyn Context,
    value: Value,
    to: Encoding,
    from: &[Encoding],
) -> NativeResult {
    let mut pending = vec![Work::Value(value)];
    let mut results = Vec::new();
    while let Some(work) = pending.pop() {
        match work {
            Work::Value(Value::Array(array)) => {
                pending.push(Work::Array(array.iter().map(|(k, _)| k.clone()).collect()));
                let values: Vec<_> = array.iter().map(|(_, v)| v.clone()).collect();
                pending.extend(values.into_iter().rev().map(Work::Value));
            }
            Work::Value(Value::String(s)) => {
                let source = if from.len() == 1 {
                    Some(from[0])
                } else {
                    guess(s.as_bytes(), from, true)
                };
                let Some(source) = source else {
                    ctx.warning(
                        "mb_convert_encoding(): Unable to detect character encoding".into(),
                    );
                    return Ok(Value::Bool(false));
                };
                results.push(text::result(ctx, &decode(s.as_bytes(), source), to)?);
            }
            Work::Value(other) => results.push(other),
            Work::Array(keys) => {
                let mut array = PhpArray::new();
                let values = results.split_off(results.len() - keys.len());
                for (k, v) in keys.into_iter().zip(values) {
                    array.insert(k, v);
                }
                results.push(Value::from(array));
            }
        }
    }
    Ok(results.pop().unwrap_or(Value::Null))
}
