//! Settings persist only within a request: `mb_internal_encoding('ASCII')` returns true.
use crate::{
    encode::Substitute,
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
    Ok(list(Encoding::all().map(|e| Value::from(e.name()))))
}
pub fn aliases(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let e = encoding::resolve(
        &String::from_utf8_lossy(args::bytes(a, 0)),
        "mb_encoding_aliases",
        1,
        "encoding",
    )?;
    crate::pseudo::selected(
        ctx,
        &String::from_utf8_lossy(args::bytes(a, 0)),
        e,
        "mb_encoding_aliases",
    );
    Ok(list(
        e.metadata()
            .1
            .split(',')
            .filter(|s| !s.is_empty())
            .map(Value::from),
    ))
}
pub fn mime(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let e = encoding::resolve(
        &String::from_utf8_lossy(args::bytes(a, 0)),
        "mb_preferred_mime_name",
        1,
        "encoding",
    )?;
    crate::pseudo::selected(
        ctx,
        &String::from_utf8_lossy(args::bytes(a, 0)),
        e,
        "mb_preferred_mime_name",
    );
    Ok(Value::from(e.metadata().2))
}
#[derive(Clone, Copy)]
pub enum Setting {
    Internal,
    Output,
    Regex,
}
pub fn encoding(ctx: &mut dyn Context, a: &[Value], name: &str, setting: Setting) -> NativeResult {
    let value = match args::string(a, 0) {
        None => None,
        Some(value) => {
            let value = value.to_string_lossy();
            let pass = value.is_empty() || value.eq_ignore_ascii_case("pass");
            if matches!(setting, Setting::Output) && pass {
                Some(Encoding(usize::MAX))
            } else {
                let mut encoding = encoding::resolve(&value, name, 1, "encoding")?;
                if matches!(setting, Setting::Regex) {
                    if encoding.0 >= 44 {
                        return Err(NativeError::value_error(format!(
                            "{name}(): Argument #1 ($encoding) must be a valid encoding, \
                             \"{value}\" given"
                        )));
                    }
                    if matches!(encoding.0, 18 | 19) {
                        encoding = Encoding(17);
                    }
                }
                Some(encoding)
            }
        }
    };
    let state = ctx.state().get_or_default::<Settings>();
    let slot = match setting {
        Setting::Internal => &mut state.internal,
        Setting::Output => &mut state.output,
        Setting::Regex => &mut state.regex,
    };
    match value {
        Some(e) => {
            *slot = e;
            Ok(Value::Bool(true))
        }
        None => Ok(Value::from(slot.name())),
    }
}
pub fn substitute(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let state = ctx.state().get_or_default::<Settings>();
    let cp = state.substitute_codepoint;
    if let Some(n) = args::int(a, 0) {
        if !(0..=0x10ffff).contains(&n) || (0xd800..=0xdfff).contains(&n) {
            return Err(NativeError::value_error(
                "mb_substitute_character(): Argument #1 ($substitute_character) \
                 is not a valid codepoint",
            ));
        }
        state.substitute_codepoint = n as u32;
    }
    let slot = &mut state.substitute;
    let new = match args::value(a, 0) {
        Value::Null => {
            return Ok(match slot {
                Substitute::Character(cp) => Value::Int(i64::from(*cp)),
                Substitute::None => Value::from("none"),
                Substitute::Long(_) => Value::from("long"),
                Substitute::Entity(_) => Value::from("entity"),
            });
        }
        Value::Int(cp) if (0..=0x10ffff).contains(cp) && !(0xd800..=0xdfff).contains(cp) => {
            Some(Substitute::Character(*cp as u32))
        }
        Value::String(s) => match s.to_string_lossy().to_ascii_lowercase().as_str() {
            "none" => Some(Substitute::None),
            "long" => Some(Substitute::Long(cp)),
            "entity" => Some(Substitute::Entity(cp)),
            _ => None,
        },
        _ => None,
    }
    .ok_or_else(|| {
        NativeError::value_error(concat!(
            "mb_substitute_character(): Argument #1 ($substitute_character) ",
            "must be \"none\", \"long\", \"entity\" or a valid codepoint"
        ))
    })?;
    *slot = new;
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
