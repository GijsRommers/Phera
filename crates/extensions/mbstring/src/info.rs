//! Inspect request settings: `mb_get_info('internal_encoding') === 'UTF-8'`.
use crate::{
    codec::{decode, encode},
    settings,
    state::Settings,
};
use phera_extension_api::{ArrayKey, Context, NativeResult, PhpArray, Value, args};
pub fn get(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let s = ctx.state().get_or_default::<Settings>().clone();
    let substitute = settings::substitute(ctx, &[])?;
    let mut all = PhpArray::new();
    for (key, value) in [
        ("internal_encoding", Value::from(s.internal.name())),
        (
            "http_output",
            Value::from(s.output.map_or("pass", |e| e.name())),
        ),
        (
            "http_output_conv_mimetypes",
            Value::from("^(text/|application/xhtml\\+xml)"),
        ),
        ("mail_charset", Value::from(mail_charset(s.language))),
        ("mail_header_encoding", Value::from("BASE64")),
        ("mail_body_encoding", Value::from("BASE64")),
        ("illegal_chars", Value::Int(0)),
        ("encoding_translation", Value::from("Off")),
        ("language", Value::from(s.language)),
        (
            "detect_order",
            settings::list(s.detect.iter().map(|e| Value::from(e.name()))),
        ),
        ("substitute_character", substitute),
        ("strict_detection", Value::from("Off")),
    ] {
        all.insert(ArrayKey::str(key), value);
    }
    let name = args::string(a, 0)
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|| "all".into());
    if name == "all" {
        return Ok(Value::from(all));
    }
    if name == "http_input" {
        return Ok(Value::Null);
    }
    if let Some(v) = all.get(&ArrayKey::str(name.as_str())) {
        return Ok(v.clone());
    }
    ctx.warning("mb_get_info(): argument #1 ($type) must be a valid type".into());
    Ok(Value::Bool(false))
}
pub fn mail_charset(language: &str) -> &'static str {
    match language {
        "Japanese" => "ISO-2022-JP",
        "English" | "German" => "ISO-8859-1",
        "Russian" => "KOI8-R",
        "Korean" => "ISO-2022-KR",
        _ => "UTF-8",
    }
}
pub fn output(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let s = ctx.state().get_or_default::<Settings>().clone();
    let Some(output) = s.output else {
        return Ok(args::value(a, 0).clone());
    };
    Ok(Value::from(encode(
        &decode(args::bytes(a, 0), s.internal),
        output,
        &s.substitute,
    )))
}
pub fn variables(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let to = crate::encoding::resolve(
        &String::from_utf8_lossy(args::bytes(a, 0)),
        "mb_convert_variables",
        1,
        "to_encoding",
    )?;
    let from = crate::conversion::encoding_list(
        ctx,
        args::value(a, 1),
        "mb_convert_variables",
        2,
        "from_encoding",
    )?;
    let mut combined = Vec::new();
    for value in &a[2..] {
        let Ok(strings) = crate::walk::strings(value.clone(), false) else {
            ctx.warning("mb_convert_variables(): Cannot handle recursive references".into());
            return Ok(Value::Bool(false));
        };
        for string in strings {
            combined.extend_from_slice(&string);
        }
    }
    let Some(from) = crate::conversion::guess(&combined, &from, false) else {
        return Ok(Value::Bool(false));
    };
    for i in 2..a.len() {
        let converted = crate::conversion::transform(ctx, a[i].deref().clone(), to, &[from])?;
        if let Some(cell) = args::reference(a, i) {
            *cell.borrow_mut() = converted;
        }
    }
    Ok(Value::from(from.name()))
}
