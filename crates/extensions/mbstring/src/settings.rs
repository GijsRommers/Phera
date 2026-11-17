//! The request's settings, which the other functions fall back on:
//!
//! ```php
//! mb_internal_encoding('ISO-8859-1');
//! mb_strlen('é');                 // 2: now read as ISO-8859-1
//! mb_substitute_character('none');
//! mb_convert_encoding('aé', 'ASCII', 'UTF-8');  // "a"
//! mb_language('Japanese');
//! mb_get_info('mail_charset');    // "ISO-2022-JP"
//! ```
//!
//! [`crate::state`] holds them for the length of a request.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

use crate::codec::{Substitute, decode, encode};
use crate::encoding::{self, Encoding};
use crate::names::list;
use crate::state::{Language, Settings};

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "mb_internal_encoding",
        ONE_ENCODING,
        mb_internal_encoding,
    ));
    registry.function(FunctionEntry::sync(
        "mb_http_output",
        ONE_ENCODING,
        mb_http_output,
    ));
    registry.function(FunctionEntry::sync(
        "mb_regex_encoding",
        ONE_ENCODING,
        mb_regex_encoding,
    ));
    registry.function(FunctionEntry::sync(
        "mb_substitute_character",
        MB_SUBSTITUTE_CHARACTER,
        mb_substitute_character,
    ));
    registry.function(FunctionEntry::sync("mb_language", MB_LANGUAGE, mb_language));
    registry.function(FunctionEntry::sync(
        "mb_http_input",
        MB_HTTP_INPUT,
        mb_http_input,
    ));
    registry.function(FunctionEntry::sync("mb_get_info", MB_GET_INFO, mb_get_info));
    registry.function(FunctionEntry::sync(
        "mb_output_handler",
        MB_OUTPUT_HANDLER,
        mb_output_handler,
    ));
}

const ONE_ENCODING: &[Param] = &[Param::new("encoding", ParamType::String)
    .nullable()
    .optional()];

/// `mb_internal_encoding(?string $encoding = null): string|bool`: the
/// encoding of strings when a function is not given one.
fn mb_internal_encoding(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
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

/// `mb_http_output(?string $encoding = null): string|bool`: what
/// `mb_output_handler` converts to; `"pass"` (or an empty string) leaves
/// output alone.
fn mb_http_output(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
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

/// `mb_regex_encoding(?string $encoding = null): string|bool`. Phera has no `mb_ereg*` functions, so this is only
/// remembered. PHP's regex engine knows no transfer encodings, and treats
/// the Microsoft variants of Shift_JIS as Shift_JIS.
fn mb_regex_encoding(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
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

const MB_SUBSTITUTE_CHARACTER: &[Param] =
    &[Param::new("substitute_character", ParamType::IntOrString)
        .nullable()
        .optional()];

/// `mb_substitute_character(string|int|null $substitute_character = null): string|int|bool`:
/// a code point, `"none"`, `"long"` or `"entity"`. The last code point set
/// stays in use for invalid bytes under `"long"` and `"entity"`.
fn mb_substitute_character(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
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
const MB_LANGUAGE: &[Param] = &[Param::new("language", ParamType::String)
    .nullable()
    .optional()];

/// `mb_language(?string $language = null): string|bool`
fn mb_language(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let Some(name) = args::string(a, 0) else {
        let current = ctx.state().get_or_default::<Settings>().language;
        return Ok(Value::from(current.name()));
    };
    let name = name.to_string_lossy();
    let Some(language) = Language::parse(&name) else {
        return Err(NativeError::value_error(format!(
            "mb_language(): Argument #1 ($language) must be a valid language, \"{name}\" given"
        )));
    };
    ctx.state().get_or_default::<Settings>().language = language;
    Ok(Value::Bool(true))
}

const MB_HTTP_INPUT: &[Param] = &[Param::new("type", ParamType::String).nullable().optional()];

/// `mb_http_input(?string $type = null): array|string|false`: Phera does
/// not translate request input, so there is never an input encoding to
/// report.
fn mb_http_input(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let kind = args::string(a, 0).map(|kind| kind.to_string_lossy().to_uppercase());
    match kind.as_deref() {
        None | Some("G" | "P" | "C" | "S" | "I") => Ok(Value::Bool(false)),
        Some("L") => Ok(Value::from("")),
        _ => Err(NativeError::value_error(concat!(
            "mb_http_input(): Argument #1 ($type) ",
            "must be one of \"G\", \"P\", \"C\", \"S\", \"I\", or \"L\""
        ))),
    }
}

const MB_GET_INFO: &[Param] = &[Param::new("type", ParamType::String).optional()];

/// `mb_get_info(string $type = "all"): array|string|int|false`
fn mb_get_info(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let settings = ctx.state().get_or_default::<Settings>().clone();
    let substitute = mb_substitute_character(ctx, &[])?;
    let detect_order = list(
        settings
            .detect
            .iter()
            .map(|encoding| Value::from(encoding.name())),
    );
    let mut all = PhpArray::new();
    for (key, value) in [
        ("internal_encoding", Value::from(settings.internal.name())),
        (
            "http_output",
            Value::from(settings.output.map_or("pass", Encoding::name)),
        ),
        (
            "http_output_conv_mimetypes",
            Value::from("^(text/|application/xhtml\\+xml)"),
        ),
        (
            "mail_charset",
            Value::from(settings.language.mail_charset()),
        ),
        ("mail_header_encoding", Value::from("BASE64")),
        ("mail_body_encoding", Value::from("BASE64")),
        ("illegal_chars", Value::Int(0)),
        ("encoding_translation", Value::from("Off")),
        ("language", Value::from(settings.language.name())),
        ("detect_order", detect_order),
        ("substitute_character", substitute),
        ("strict_detection", Value::from("Off")),
    ] {
        all.insert(ArrayKey::str(key), value);
    }
    let kind = args::string(a, 0).map_or_else(|| "all".into(), |kind| kind.to_string_lossy());
    match kind.as_str() {
        "all" => return Ok(Value::from(all)),
        "http_input" => return Ok(Value::Null),
        _ => {}
    }
    if let Some(value) = all.get(&ArrayKey::str(kind.as_str())) {
        return Ok(value.clone());
    }
    ctx.warning("mb_get_info(): argument #1 ($type) must be a valid type".into());
    Ok(Value::Bool(false))
}

const MB_OUTPUT_HANDLER: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("status", ParamType::Int),
];

/// `mb_output_handler(string $string, int $status): string`: an output
/// buffer callback converting from the internal encoding to the HTTP output
/// encoding.
fn mb_output_handler(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let settings = ctx.state().get_or_default::<Settings>();
    let Some(output) = settings.output else {
        return Ok(args::value(a, 0).clone());
    };
    let points = decode(args::bytes(a, 0), settings.internal);
    Ok(Value::from(encode(&points, output, &settings.substitute)))
}
