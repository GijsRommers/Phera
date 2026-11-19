//! PHP XML parser options select callback names, whitespace and target encoding.
//!
//! ```php
//! xml_parser_set_option($parser, XML_OPTION_CASE_FOLDING, false);
//! xml_parser_set_option($parser, XML_OPTION_SKIP_WHITE, true);
//! ```

use phera_extension_api::{
    Context, NativeError, Value,
    value::{to_bool, to_int},
};

#[derive(Clone, Copy)]
pub enum Codec {
    Utf8,
    Latin1,
    Ascii,
}

impl Codec {
    pub fn from_label(label: &str) -> Option<Self> {
        match label.to_ascii_uppercase().as_str() {
            "UTF-8" => Some(Self::Utf8),
            "ISO-8859-1" => Some(Self::Latin1),
            "US-ASCII" => Some(Self::Ascii),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Latin1 => "ISO-8859-1",
            Self::Ascii => "US-ASCII",
        }
    }

    pub fn encode(self, text: &str) -> Vec<u8> {
        let maximum = match self {
            Self::Utf8 => return text.as_bytes().to_vec(),
            Self::Latin1 => 255,
            Self::Ascii => 127,
        };
        text.chars()
            .map(|character| {
                if character as u32 > maximum {
                    b'?'
                } else {
                    character as u8
                }
            })
            .collect()
    }
}

pub struct Options {
    pub source: Option<Codec>,
    pub target: Codec,
    pub case_fold: bool,
    pub skip_tag_start: usize,
    pub skip_white: bool,
    pub huge: bool,
}

impl Options {
    pub fn new(source: Option<Codec>) -> Self {
        Self {
            source,
            target: source.unwrap_or(Codec::Utf8),
            case_fold: true,
            skip_tag_start: 0,
            skip_white: false,
            huge: false,
        }
    }
}

#[derive(Clone, Copy)]
pub enum OptionName {
    CaseFolding,
    TargetEncoding,
    SkipTagStart,
    SkipWhite,
    Huge,
}

impl OptionName {
    pub fn read(value: i64, function: &str) -> Result<Self, NativeError> {
        match value {
            1 => Ok(Self::CaseFolding),
            2 => Ok(Self::TargetEncoding),
            3 => Ok(Self::SkipTagStart),
            4 => Ok(Self::SkipWhite),
            5 => Ok(Self::Huge),
            _ => Err(NativeError::value_error(format!(
                "{function}(): Argument #2 ($option) must be a XML_OPTION_* constant"
            ))),
        }
    }

    pub fn get(self, options: &Options) -> Value {
        match self {
            Self::CaseFolding => Value::Bool(options.case_fold),
            Self::TargetEncoding => Value::from(options.target.label()),
            Self::SkipTagStart => Value::Int(options.skip_tag_start as i64),
            Self::SkipWhite => Value::Bool(options.skip_white),
            Self::Huge => Value::Bool(options.huge),
        }
    }

    pub fn set(
        self,
        options: &mut Options,
        context: &mut dyn Context,
        value: &Value,
    ) -> Result<bool, NativeError> {
        if !matches!(value, Value::String(_) | Value::Int(_) | Value::Bool(_)) {
            context.warning(format!(
                "xml_parser_set_option(): Argument #3 ($value) must be of type string|int|bool, {} given",
                value.type_name()
            ));
        }
        match self {
            Self::CaseFolding => options.case_fold = to_bool(value),
            Self::SkipWhite => options.skip_white = to_bool(value),
            Self::Huge => options.huge = to_bool(value),
            Self::SkipTagStart => {
                let offset = to_int(value, context.diagnostics())?;
                if !(0..=i64::from(i32::MAX)).contains(&offset) {
                    context.warning(
                        concat!(
                            "xml_parser_set_option(): Argument #3 ($value) must be between ",
                            "0 and 2147483647 for option XML_OPTION_SKIP_TAGSTART"
                        )
                        .into(),
                    );
                    return Ok(false);
                }
                options.skip_tag_start = offset as usize;
            }
            Self::TargetEncoding => {
                let label =
                    phera_extension_api::value::to_php_string(value, context.diagnostics())?;
                options.target = Codec::from_label(&label.to_string_lossy()).ok_or_else(|| {
                    NativeError::value_error(
                        "xml_parser_set_option(): Argument #3 ($value) is not a supported target encoding",
                    )
                })?;
            }
        }
        Ok(true)
    }
}
