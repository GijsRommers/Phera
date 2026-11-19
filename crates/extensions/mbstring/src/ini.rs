//! `ini_set('default_charset', 'ISO-8859-1')` changes mbstring's default
//! encoding unless an mbstring INI setting supplies a more specific choice.

use phera_extension_api::{Context, Registry, ini};

use crate::codec::Substitute;
use crate::encoding::Encoding;
use crate::state::{Language, Settings};

pub(crate) fn register(registry: &mut Registry) {
    for name in ["default_charset", "internal_encoding", "output_encoding"] {
        registry.ini_update(name, |context, name, value, function| {
            Ok(update_default(context, name, value, function))
        });
    }
    for name in [
        "mbstring.internal_encoding",
        "mbstring.http_output",
        "mbstring.http_input",
        "mbstring.language",
        "mbstring.detect_order",
        "mbstring.substitute_character",
    ] {
        registry.ini_update(name, |context, name, value, function| {
            Ok(update_mbstring(context, name, value, function))
        });
    }
}

fn update_default(
    context: &mut dyn Context,
    name: &str,
    value: Option<&[u8]>,
    function: &str,
) -> bool {
    let charset = value.filter(|bytes| !bytes.is_empty());
    let override_for =
        |context: &dyn Context, key: &[u8]| context.ini_get(key).filter(|value| !value.is_empty());
    let default = override_for(context, b"default_charset");
    let fallback = if name == "default_charset" {
        charset.unwrap_or(b"UTF-8")
    } else {
        default
            .as_ref()
            .map_or(b"UTF-8".as_slice(), |value| value.as_bytes())
    };
    let internal = override_for(context, b"internal_encoding");
    let output = override_for(context, b"output_encoding");
    let internal = if name == "internal_encoding" {
        charset.unwrap_or(fallback)
    } else {
        internal.as_ref().map_or(fallback, |value| value.as_bytes())
    };
    let output = if name == "output_encoding" {
        charset.unwrap_or(fallback)
    } else {
        output.as_ref().map_or(fallback, |value| value.as_bytes())
    };

    if override_for(context, b"mbstring.internal_encoding").is_none() {
        let internal = encoding_or_warn(context, function, internal);
        let settings = context.state().get_or_default::<Settings>();
        settings.internal = internal;
    }
    if override_for(context, b"mbstring.http_output").is_none() {
        let output = String::from_utf8_lossy(output);
        if let Some(encoding) = Encoding::find(&output) {
            context.state().get_or_default::<Settings>().output = Some(encoding);
        } else if output.eq_ignore_ascii_case("pass") {
            context.state().get_or_default::<Settings>().output = None;
        } else {
            context.warning(format!(
                "{function}(): INI setting contains invalid encoding \"{output}\""
            ));
        }
    }
    true
}

fn update_mbstring(
    context: &mut dyn Context,
    name: &str,
    value: Option<&[u8]>,
    function: &str,
) -> bool {
    let text = String::from_utf8_lossy(value.unwrap_or_default());
    if value.is_some()
        && matches!(
            name,
            "mbstring.internal_encoding" | "mbstring.http_input" | "mbstring.http_output"
        )
    {
        context.deprecated(format!("{function}(): Use of {name} is deprecated"));
    }
    match name {
        "mbstring.internal_encoding" => {
            let fallback;
            let encoding = if text.is_empty() {
                fallback = ini::encoding(context, b"internal_encoding");
                fallback.as_bytes()
            } else {
                text.as_bytes()
            };
            context.state().get_or_default::<Settings>().internal =
                encoding_or_warn(context, function, encoding);
        }
        "mbstring.http_output" => {
            let fallback;
            let encoding = if text.is_empty() {
                fallback = ini::encoding(context, b"output_encoding").to_string_lossy();
                fallback.as_str()
            } else {
                text.as_ref()
            };
            let output = if encoding.eq_ignore_ascii_case("pass") {
                None
            } else if let Some(encoding) = Encoding::find(encoding) {
                Some(encoding)
            } else {
                context.warning(format!(
                    "{function}(): INI setting contains invalid encoding \"{encoding}\""
                ));
                return false;
            };
            context.state().get_or_default::<Settings>().output = output;
        }
        "mbstring.language" => {
            let Some(language) = Language::parse(&text) else {
                context.state().get_or_default::<Settings>().language = Language::Neutral;
                return false;
            };
            context.state().get_or_default::<Settings>().language = language;
        }
        "mbstring.detect_order" => {
            let detect = if value.is_none() {
                Settings::default().detect
            } else {
                let Some(detect) = encoding_list(context, function, &text) else {
                    return false;
                };
                detect
            };
            context.state().get_or_default::<Settings>().detect = detect;
        }
        "mbstring.http_input" if !text.is_empty() && text != "pass" => {
            return encoding_list(context, function, &text).is_some();
        }
        "mbstring.substitute_character" => {
            let settings = context.state().get_or_default::<Settings>();
            if value.is_none() {
                settings.substitute_character = '?';
            }
            settings.substitute = match text.to_ascii_lowercase().as_str() {
                "none" => Substitute::None,
                "long" => Substitute::Long(settings.substitute_character),
                "entity" => Substitute::Entity(settings.substitute_character),
                _ => {
                    let (point, diagnostic) = ini::parse_quantity(text.as_bytes());
                    if !text.is_empty()
                        && diagnostic.is_none()
                        && let Some(character) = u32::try_from(point).ok().and_then(char::from_u32)
                    {
                        settings.substitute_character = character;
                    }
                    Substitute::Character(settings.substitute_character)
                }
            };
        }
        _ => {}
    }
    true
}

fn encoding_or_warn(context: &mut dyn Context, function: &str, input: &[u8]) -> Encoding {
    let encoding = String::from_utf8_lossy(input.split(|byte| *byte == 0).next().unwrap_or(b""));
    Encoding::find(&encoding).unwrap_or_else(|| {
        context.warning(format!(
            "{function}(): Unknown encoding \"{encoding}\" in ini setting"
        ));
        Encoding::Utf8
    })
}

fn encoding_list(context: &mut dyn Context, function: &str, input: &str) -> Option<Vec<Encoding>> {
    let mut result = Vec::new();
    for name in input.trim_matches('"').split(',').map(str::trim) {
        if name.eq_ignore_ascii_case("auto") {
            result.extend(Settings::default().detect);
        } else if let Some(encoding) = Encoding::find(name) {
            result.push(encoding);
        } else if !name.is_empty() {
            context.warning(format!("{function}(): Unknown encoding \"{name}\""));
            return None;
        }
    }
    (!result.is_empty()).then_some(result)
}
