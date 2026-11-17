//! Settings persist only within a request: `mb_internal_encoding('ASCII')` returns true.
use crate::{
    codec::Substitute,
    encoding::{self, Encoding},
    state::Settings,
};
use phera_extension_api::{Context, NativeError, NativeResult, PhpArray, Value, args};

pub fn list(values: impl IntoIterator<Item = Value>) -> Value {
    let mut array = PhpArray::new();
    for value in values {
        let _ = array.push(value);
    }
    Value::from(array)
}
pub fn encodings(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(list(Encoding::ALL.iter().map(|e| Value::from(e.name()))))
}
pub fn aliases(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let e = encoding::named(
        ctx,
        &String::from_utf8_lossy(args::bytes(a, 0)),
        "mb_encoding_aliases",
        1,
        "encoding",
    )?;
    Ok(list(e.aliases().iter().map(|alias| Value::from(*alias))))
}
pub fn mime(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let e = encoding::named(
        ctx,
        &String::from_utf8_lossy(args::bytes(a, 0)),
        "mb_preferred_mime_name",
        1,
        "encoding",
    )?;
    Ok(Value::from(e.mime_name()))
}
/// `mb_internal_encoding()`: the encoding of strings when a function is not
/// given one.
pub fn internal_encoding(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let Some(name) = args::string(a, 0) else {
        let current = ctx.state().get_or_default::<Settings>().internal;
        return Ok(Value::from(current.name()));
    };
    let encoding = encoding::resolve(
        &name.to_string_lossy(),
        "mb_internal_encoding",
        1,
        "encoding",
    )?;
    ctx.state().get_or_default::<Settings>().internal = encoding;
    Ok(Value::Bool(true))
}

/// `mb_http_output()`: what `mb_output_handler` converts to; `"pass"` (or an
/// empty string) leaves output alone.
pub fn http_output(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let Some(name) = args::string(a, 0) else {
        let current = ctx.state().get_or_default::<Settings>().output;
        return Ok(Value::from(current.map_or("pass", Encoding::name)));
    };
    let name = name.to_string_lossy();
    let output = if name.is_empty() || name.eq_ignore_ascii_case("pass") {
        None
    } else {
        Some(encoding::resolve(&name, "mb_http_output", 1, "encoding")?)
    };
    ctx.state().get_or_default::<Settings>().output = output;
    Ok(Value::Bool(true))
}

/// `mb_regex_encoding()`. Phera has no `mb_ereg*` functions, so this is only
/// remembered. PHP's regex engine knows no transfer encodings, and treats
/// the Microsoft variants of Shift_JIS as Shift_JIS.
pub fn regex_encoding(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let Some(name) = args::string(a, 0) else {
        let current = ctx.state().get_or_default::<Settings>().regex;
        return Ok(Value::from(current.name()));
    };
    let name = name.to_string_lossy();
    let encoding = match encoding::resolve(&name, "mb_regex_encoding", 1, "encoding")? {
        Encoding::Cp932 | Encoding::SjisWin => Encoding::Sjis,
        transfer if transfer.transfer().is_some() => {
            return Err(NativeError::value_error(format!(
                "mb_regex_encoding(): Argument #1 ($encoding) must be a valid encoding, \"{name}\" given"
            )));
        }
        other => other,
    };
    ctx.state().get_or_default::<Settings>().regex = encoding;
    Ok(Value::Bool(true))
}

/// `mb_substitute_character()`: a code point, `"none"`, `"long"` or
/// `"entity"`. The last code point set stays in use for invalid bytes under
/// `"long"` and `"entity"`.
pub fn substitute(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let state = ctx.state().get_or_default::<Settings>();
    if let Some(point) = args::int(a, 0) {
        let Some(character) = u32::try_from(point).ok().and_then(char::from_u32) else {
            return Err(NativeError::value_error(
                "mb_substitute_character(): Argument #1 ($substitute_character) \
                 is not a valid codepoint",
            ));
        };
        state.substitute_character = character;
        state.substitute = Substitute::Character(character);
        return Ok(Value::Bool(true));
    }
    let Some(name) = args::string(a, 0) else {
        return Ok(match state.substitute {
            Substitute::Character(character) => Value::Int(i64::from(u32::from(character))),
            Substitute::None => Value::from("none"),
            Substitute::Long(_) => Value::from("long"),
            Substitute::Entity(_) => Value::from("entity"),
        });
    };
    let stored = state.substitute_character;
    state.substitute = match name.to_string_lossy().to_ascii_lowercase().as_str() {
        "none" => Substitute::None,
        "long" => Substitute::Long(stored),
        "entity" => Substitute::Entity(stored),
        _ => {
            return Err(NativeError::value_error(concat!(
                "mb_substitute_character(): Argument #1 ($substitute_character) ",
                "must be \"none\", \"long\", \"entity\" or a valid codepoint"
            )));
        }
    };
    Ok(Value::Bool(true))
}
pub fn language(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let slot = &mut ctx.state().get_or_default::<Settings>().language;
    let Some(s) = args::string(a, 0) else {
        return Ok(Value::from(*slot));
    };
    let name = s.to_string_lossy();
    *slot = match name.to_ascii_lowercase().as_str() {
        "neutral" => "neutral",
        "uni" => "uni",
        "universal" => "uni",
        "japanese" | "ja" => "Japanese",
        "english" | "en" => "English",
        "german" | "de" => "German",
        "russian" | "ru" => "Russian",
        "korean" | "ko" => "Korean",
        "armenian" | "hy" => "Armenian",
        "turkish" | "tr" => "Turkish",
        "ukrainian" | "ua" => "Ukrainian",
        "simplified chinese" | "zh-cn" => "Simplified Chinese",
        "traditional chinese" | "zh-tw" => "Traditional Chinese",
        _ => {
            return Err(NativeError::value_error(format!(
                "mb_language(): Argument #1 ($language) must be a valid language, \"{name}\" given"
            )));
        }
    };
    Ok(Value::Bool(true))
}
pub fn input(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let name = args::string(a, 0).map(|s| s.to_string_lossy().to_uppercase());
    match name.as_deref() {
        None | Some("G" | "P" | "C" | "S" | "I") => Ok(Value::Bool(false)),
        Some("L") => Ok(Value::from("")),
        _ => Err(NativeError::value_error(concat!(
            "mb_http_input(): Argument #1 ($type) ",
            "must be one of \"G\", \"P\", \"C\", \"S\", \"I\", or \"L\""
        ))),
    }
}
