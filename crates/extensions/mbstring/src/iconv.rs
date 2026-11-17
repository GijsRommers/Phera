//! Native iconv retains platform mappings: `iconv('UTF-8', 'ASCII//TRANSLIT', 'é')`.
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, args};
#[derive(Clone)]
pub struct Settings {
    encodings: [String; 3],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            encodings: std::array::from_fn(|_| "UTF-8".into()),
        }
    }
}
const NAMES: [&str; 3] = ["input_encoding", "output_encoding", "internal_encoding"];
pub fn convert_bytes(
    ctx: &mut dyn Context,
    bytes: &[u8],
    from: &str,
    to: &str,
    name: &str,
) -> Option<Vec<u8>> {
    let result = if to.to_ascii_uppercase().contains("//IGNORE") {
        let probe = iconv_native::convert(bytes, from, "UCS-4LE//IGNORE");
        if matches!(probe, Err(iconv_native::ConvertError::InvalidInput))
            && std::io::Error::last_os_error().raw_os_error() == Some(22)
        {
            probe
        } else {
            iconv_native::convert_lossy(bytes, from, to).map_err(Into::into)
        }
    } else {
        iconv_native::convert(bytes, from, to)
    };
    match result {
        Ok(bytes) => Some(bytes),
        Err(iconv_native::ConvertError::UnknownConversion) => {
            ctx.warning(format!(
                "{name}(): Wrong encoding, conversion from \"{from}\" to \"{to}\" is not allowed"
            ));
            None
        }
        Err(iconv_native::ConvertError::InvalidInput) => {
            let incomplete = std::io::Error::last_os_error().raw_os_error() == Some(22);
            ctx.notice(format!(
                "{name}(): Detected an {} in input string",
                if incomplete {
                    "incomplete multibyte character"
                } else {
                    "illegal character"
                }
            ));
            None
        }
    }
}
pub fn convert(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let from = String::from_utf8_lossy(args::bytes(a, 0));
    let to = String::from_utf8_lossy(args::bytes(a, 1));
    Ok(convert_bytes(ctx, args::bytes(a, 2), &from, &to, "iconv")
        .map_or(Value::Bool(false), Value::from))
}
pub fn encoding(ctx: &mut dyn Context, a: &[Value], index: usize) -> String {
    args::string(a, index)
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|| ctx.state().get_or_default::<Settings>().encodings[2].clone())
}
fn points(
    ctx: &mut dyn Context,
    a: &[Value],
    index: usize,
    name: &str,
) -> Option<(Vec<u32>, String)> {
    let encoding = encoding(ctx, a, index);
    let bytes = convert_bytes(ctx, args::bytes(a, 0), &encoding, "UCS-4LE", name)?;
    Some((
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| crate::encoding::read_u32(bytes))
            .collect(),
        encoding,
    ))
}
pub fn strlen(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(points(ctx, a, 1, "iconv_strlen")
        .map_or(Value::Bool(false), |(p, _)| Value::Int(p.len() as i64)))
}
pub fn substr(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let Some((p, e)) = points(ctx, a, 3, "iconv_substr") else {
        return Ok(Value::Bool(false));
    };
    let (start, end) = crate::text::bounds(p.len(), args::int(a, 1).unwrap_or(0), args::int(a, 2));
    let bytes: Vec<u8> = p[start..end]
        .iter()
        .flat_map(|cp| cp.to_le_bytes())
        .collect();
    Ok(convert_bytes(ctx, &bytes, "UCS-4LE", &e, "iconv_substr")
        .map_or(Value::Bool(false), Value::from))
}
pub fn position(ctx: &mut dyn Context, a: &[Value], reverse: bool) -> NativeResult {
    let name = if reverse {
        "iconv_strrpos"
    } else {
        "iconv_strpos"
    };
    let e = encoding(ctx, a, if reverse { 2 } else { 3 });
    if !reverse
        && args::int(a, 2).unwrap_or(0) < 0
        && convert_bytes(ctx, args::bytes(a, 0), &e, "UCS-4LE", name).is_none()
    {
        return Ok(Value::Bool(false));
    }
    let converted = iconv_native::convert_lossy(args::bytes(a, 0), &e, "UCS-4LE//");
    let Ok(converted) = converted else {
        convert_bytes(ctx, args::bytes(a, 0), &e, "UCS-4LE", name);
        return Ok(Value::Bool(false));
    };
    let p: Vec<_> = converted
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| crate::encoding::read_u32(b))
        .collect();
    let offset = if reverse {
        0
    } else {
        args::int(a, 2).unwrap_or(0)
    };
    if offset < -(p.len() as i64) {
        return Err(offset_error(name));
    }
    let Some(bytes) = convert_bytes(ctx, args::bytes(a, 1), &e, "UCS-4LE", name) else {
        return Ok(Value::Bool(false));
    };
    let needle: Vec<_> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| crate::encoding::read_u32(bytes))
        .collect();
    if offset > p.len() as i64 {
        if !p.is_empty() && convert_bytes(ctx, args::bytes(a, 0), &e, "UCS-4LE", name).is_none() {
            return Ok(Value::Bool(false));
        }
        return Err(offset_error(name));
    }
    if needle.is_empty() {
        return Ok(Value::Bool(false));
    }
    let offset = if offset < 0 {
        p.len() - offset.unsigned_abs() as usize
    } else {
        offset as usize
    };
    let mut positions = (offset..p.len()).filter(|i| p[*i..].starts_with(&needle));
    let found = if reverse {
        positions.next_back()
    } else {
        positions.next()
    };
    if !p.is_empty()
        && (reverse || found.is_none())
        && convert_bytes(ctx, args::bytes(a, 0), &e, "UCS-4LE", name).is_none()
    {
        return Ok(Value::Bool(false));
    }
    Ok(found.map_or(Value::Bool(false), |i| Value::Int(i as i64)))
}

pub fn get(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let name = args::string(a, 0)
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|| "all".into());
    let state = ctx.state().get_or_default::<Settings>();
    if name.eq_ignore_ascii_case("all") {
        let mut array = PhpArray::new();
        for (name, value) in NAMES.iter().zip(&state.encodings) {
            array.insert(ArrayKey::str(*name), Value::from(value.clone()));
        }
        return Ok(Value::from(array));
    }
    Ok(NAMES
        .iter()
        .position(|n| n.eq_ignore_ascii_case(&name))
        .map_or(Value::Bool(false), |i| {
            Value::from(state.encodings[i].clone())
        }))
}
pub fn set(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let name = String::from_utf8_lossy(args::bytes(a, 0));
    let Some(i) = NAMES.iter().position(|n| n.eq_ignore_ascii_case(&name)) else {
        return Ok(Value::Bool(false));
    };
    ctx.deprecated(format!(
        "iconv_set_encoding(): Use of iconv.{} is deprecated",
        NAMES[i]
    ));
    ctx.state().get_or_default::<Settings>().encodings[i] =
        String::from_utf8_lossy(args::bytes(a, 1)).into_owned();
    Ok(Value::Bool(true))
}

fn offset_error(name: &str) -> NativeError {
    NativeError::value_error(format!(
        "{name}(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
    ))
}
