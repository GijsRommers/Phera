//! Validate fetch-mode arguments before changing a statement's fetch configuration.

use phera_extension_api::{NativeError, Value, args};

use crate::fetch::FetchMode;

#[derive(Clone, Copy)]
pub enum Method {
    Query,
    SetMode,
    All,
}

impl Method {
    fn name(self) -> &'static str {
        match self {
            Self::Query => "PDO::query",
            Self::SetMode => "PDOStatement::setFetchMode",
            Self::All => "PDOStatement::fetchAll",
        }
    }
}

pub struct Configuration {
    pub mode: FetchMode,
    pub column: usize,
    pub flags: i64,
}

pub fn validate(
    method: Method,
    arguments: &[Value],
    default: FetchMode,
    default_column: usize,
    default_flags: i64,
) -> Result<Configuration, NativeError> {
    let offset = usize::from(matches!(method, Method::Query));
    let name = method.name();
    let number = args::int(arguments, offset).unwrap_or(0);
    let argument = if offset == 1 {
        "#2 ($fetchMode)"
    } else {
        "#1 ($mode)"
    };
    if number < 0 || number & !0x3ff != 0 || number & 0x1f > 12 {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument {argument} must be a bitmask of PDO::FETCH_* constants"
        )));
    }
    if number & 0x380 != 0 && number & 0x1f != 8 {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument {argument} cannot use PDO::FETCH_CLASSTYPE, \
             PDO::FETCH_PROPS_LATE, or PDO::FETCH_SERIALIZE fetch flags with a fetch \
             mode other than PDO::FETCH_CLASS"
        )));
    }
    let mode = FetchMode::parse(number & 0x1f, default)?;
    let flags = if number == 0 {
        default_flags
    } else {
        number & 0x60
    };
    let expected = offset + 1 + usize::from(matches!(mode, FetchMode::Column));
    let count = arguments.len();
    if matches!(method, Method::All) && count > 2 {
        let third = args::value(arguments, 2);
        if !matches!(third, Value::Array(_) | Value::Null) {
            return Err(NativeError::type_error(format!(
                "{name}(): Argument #3 must be of type ?array, {} given",
                type_name(third)
            )));
        }
    }
    let valid = match method {
        Method::All => count <= expected,
        Method::Query if count <= 1 => true,
        _ => count == expected,
    };
    if !valid {
        let qualifier = if matches!(method, Method::All) && expected == 2 {
            "at most"
        } else {
            "exactly"
        };
        let noun = if matches!(method, Method::All) {
            "argument"
        } else {
            "arguments"
        };
        return Err(NativeError::argument_count_error(format!(
            "{name}() expects {qualifier} {expected} {noun} for the fetch mode provided, \
             {count} given"
        )));
    }
    let column = if matches!(mode, FetchMode::Column) && count > offset + 1 {
        let value = args::value(arguments, offset + 1);
        let column = match value {
            Value::Int(column) => *column,
            Value::String(text) => text.to_string_lossy().parse::<i64>().map_err(|_| {
                NativeError::type_error(format!(
                    "{name}(): Argument #{} must be of type int, string given",
                    offset + 2
                ))
            })?,
            _ => {
                return Err(NativeError::type_error(format!(
                    "{name}(): Argument #{} must be of type int, {} given",
                    offset + 2,
                    type_name(value)
                )));
            }
        };
        usize::try_from(column).map_err(|_| {
            NativeError::value_error(format!(
                "{name}(): Argument #{} must be greater than or equal to 0",
                offset + 2
            ))
        })?
    } else if number == 0 {
        default_column
    } else {
        0
    };
    Ok(Configuration {
        mode,
        column,
        flags,
    })
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Int(_) => "int",
        Value::String(_) => "string",
        Value::Float(_) => "float",
        Value::Bool(_) => "bool",
        Value::Null => "null",
        Value::Array(_) => "array",
        _ => "object",
    }
}
