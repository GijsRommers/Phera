//! The `iconv.*_encoding` settings for the rest of the request:
//!
//! ```php
//! iconv_get_encoding('internal_encoding');            // "UTF-8"
//! iconv_set_encoding('internal_encoding', 'ISO-8859-1'); // true, and a deprecation
//! iconv_strlen("\xE9");                               // 1: now read as ISO-8859-1
//! ```

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "iconv_get_encoding",
        ICONV_GET_ENCODING,
        iconv_get_encoding,
    ));
    registry.function(FunctionEntry::sync(
        "iconv_set_encoding",
        ICONV_SET_ENCODING,
        iconv_set_encoding,
    ));
}

/// The `iconv.*_encoding` settings, which `iconv_set_encoding` changes for
/// the rest of the request.
#[derive(Clone)]
pub(crate) struct Settings {
    input: String,
    output: String,
    internal: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            input: "UTF-8".into(),
            output: "UTF-8".into(),
            internal: "UTF-8".into(),
        }
    }
}

#[derive(Clone, Copy)]
enum Setting {
    Input,
    Output,
    Internal,
}

impl Setting {
    const ALL: [Setting; 3] = [Self::Input, Self::Output, Self::Internal];

    fn name(self) -> &'static str {
        match self {
            Self::Input => "input_encoding",
            Self::Output => "output_encoding",
            Self::Internal => "internal_encoding",
        }
    }

    fn find(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|setting| setting.name().eq_ignore_ascii_case(name))
    }

    fn of(self, settings: &mut Settings) -> &mut String {
        match self {
            Self::Input => &mut settings.input,
            Self::Output => &mut settings.output,
            Self::Internal => &mut settings.internal,
        }
    }
}

/// The encoding argument at `index`, or `iconv.internal_encoding`.
pub fn encoding_argument(ctx: &mut dyn Context, a: &[Value], index: usize) -> String {
    match args::string(a, index) {
        Some(name) => name.to_string_lossy(),
        None => ctx.state().get_or_default::<Settings>().internal.clone(),
    }
}

/// `iconv_mime_*` take this as their default target.
pub fn internal_encoding(ctx: &mut dyn Context) -> String {
    ctx.state().get_or_default::<Settings>().internal.clone()
}

const ICONV_GET_ENCODING: &[Param] = &[Param::new("type", ParamType::String).optional()];

/// `iconv_get_encoding(string $type = "all"): array|string|false`
fn iconv_get_encoding(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let kind = args::string(a, 0).map_or_else(|| "all".into(), |kind| kind.to_string_lossy());
    let settings = ctx.state().get_or_default::<Settings>();
    if kind.eq_ignore_ascii_case("all") {
        let mut all = PhpArray::new();
        for setting in Setting::ALL {
            let value = Value::from(setting.of(settings).clone());
            all.insert(ArrayKey::str(setting.name()), value);
        }
        return Ok(Value::from(all));
    }
    Ok(Setting::find(&kind).map_or(Value::Bool(false), |setting| {
        Value::from(setting.of(settings).clone())
    }))
}

const ICONV_SET_ENCODING: &[Param] = &[
    Param::new("type", ParamType::String),
    Param::new("encoding", ParamType::String),
];

/// `iconv_set_encoding(string $type, string $encoding): bool`, deprecated
/// like the settings themselves.
fn iconv_set_encoding(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let Some(setting) = Setting::find(&String::from_utf8_lossy(args::bytes(a, 0))) else {
        return Ok(Value::Bool(false));
    };
    ctx.deprecated(format!(
        "iconv_set_encoding(): Use of iconv.{} is deprecated",
        setting.name()
    ));
    let encoding = String::from_utf8_lossy(args::bytes(a, 1)).into_owned();
    *setting.of(ctx.state().get_or_default::<Settings>()) = encoding;
    Ok(Value::Bool(true))
}
