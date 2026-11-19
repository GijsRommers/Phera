//! `parse_ini_string('enabled = yes', scanner_mode: INI_SCANNER_TYPED)` returns true.
//!
//! `parse_ini_file()` performs asynchronous I/O in the streams extension and
//! hands its bytes to the same lexer and parser.

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

mod expression;
mod parser;
mod patterns;
mod scanner;
mod settings;
mod strings;
mod token;

pub const PROCESS_SECTIONS: Param =
    Param::new("process_sections", ParamType::Bool).with_default(ConstantValue::Bool(false));
pub const SCANNER_MODE: Param =
    Param::new("scanner_mode", ParamType::Int).with_default(ConstantValue::Int(0));

const MAX_DEPTH: usize = 10_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Raw,
    Typed,
}

impl Mode {
    fn from_int(mode: i64) -> Option<Self> {
        match mode as i32 {
            0 => Some(Self::Normal),
            1 => Some(Self::Raw),
            2 => Some(Self::Typed),
            _ => None,
        }
    }
}

pub(crate) fn register(registry: &mut Registry) {
    settings::register(registry);
    registry.constant("INI_SCANNER_NORMAL", 0);
    registry.constant("INI_SCANNER_RAW", 1);
    registry.constant("INI_SCANNER_TYPED", 2);
    registry.function(FunctionEntry::sync(
        "parse_ini_string",
        const {
            &[
                Param::new("ini_string", ParamType::String),
                PROCESS_SECTIONS,
                SCANNER_MODE,
            ]
        },
        parse_ini_string,
    ));
    registry.describe_function("parse_ini_string", "array|false", None);
}

fn parse_ini_string(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::string(arguments, 0).map_or(&[][..], |text| text.as_bytes());
    let input = input.split(|&byte| byte == 0).next().unwrap_or_default();
    Ok(parse(context, input, "Unknown", arguments))
}

/// Parse bytes already read by the caller; a bad document emits a warning and returns false.
pub fn parse(
    context: &mut dyn Context,
    input: &[u8],
    filename: &str,
    arguments: &[Value],
) -> Value {
    let Some(mode) = Mode::from_int(args::int(arguments, 2).unwrap_or_default()) else {
        context.warning("Invalid scanner mode".into());
        return Value::Bool(false);
    };
    let sections = args::bool(arguments, 1).unwrap_or_default();
    let mut parser = parser::Parser::new(input, mode);
    match parser.parse(context, sections) {
        Ok(values) => Value::from(values),
        Err(message) => {
            context.warning(format!(
                "{message} in {filename} on line {}\n",
                parser.line()
            ));
            Value::Bool(false)
        }
    }
}
