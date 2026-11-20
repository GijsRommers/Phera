//! `pg_convert($db, $table, $fields)` converts PHP values to SQL fragments.

use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, Value, value::to_php_string};

use super::metadata::Column;
use crate::escape;

pub struct Field {
    pub name: String,
    pub value: Value,
}

pub fn convert(
    context: &mut dyn Context,
    values: Option<&PhpArray>,
    columns: &[Column],
    flags: i64,
    call: &str,
) -> Result<Option<Vec<Field>>, NativeError> {
    let Some(values) = values else {
        return Ok(Some(Vec::new()));
    };
    let mut fields = Vec::with_capacity(values.len());
    for (key, value) in values.iter() {
        let ArrayKey::String(name) = key else {
            context.warning(format!("{call}(): Accepts only associative array"));
            return Ok(None);
        };
        let name = name.to_string_lossy();
        let column = columns.iter().find(|column| column.name == name);
        let value = value.deref();
        let no_conversion = flags & 256 != 0;
        let force_escape = flags & 4096 != 0;
        let converted = if no_conversion {
            Value::from(
                to_php_string(&value, context.diagnostics())?
                    .as_bytes()
                    .to_vec(),
            )
        } else if force_escape {
            Value::from(escape::literal(
                &to_php_string(&value, context.diagnostics())?.to_string_lossy(),
            ))
        } else {
            let Some(column) = column else {
                context.warning(format!("{call}(): Invalid field name ({name}) in values"));
                return Ok(None);
            };
            match convert_value(context, &value, column, flags)? {
                Some(value) => value,
                None => {
                    if (matches!(&*value, Value::Null)
                        || matches!(&*value, Value::String(s) if s.as_bytes().is_empty() && flags & 4 != 0))
                        && column.default
                        && flags & 2 == 0
                    {
                        continue;
                    }
                    context.warning(format!("{call}(): Invalid value for field {name}"));
                    return Ok(None);
                }
            }
        };
        fields.push(Field {
            name: escape::identifier(&name),
            value: converted,
        });
    }
    Ok(Some(fields))
}

fn convert_value(
    context: &mut dyn Context,
    value: &Value,
    column: &Column,
    flags: i64,
) -> Result<Option<Value>, NativeError> {
    let empty = matches!(value, Value::String(s) if s.as_bytes().is_empty());
    if matches!(value, Value::Null) || empty && flags & 4 != 0 {
        return Ok(
            ((!column.not_null || flags & 8 != 0) && (!column.default || flags & 2 != 0))
                .then(|| Value::from("NULL")),
        );
    }
    let text = to_php_string(value, context.diagnostics())?.to_string_lossy();
    let converted = match column.type_name.as_str() {
        "int2" | "int4" | "int8" | "oid" => match value {
            Value::Int(number) => Some(Value::Int(*number)),
            Value::Float(number) => Some(Value::Int(*number as i64)),
            Value::String(_) if text.is_empty() => Some(Value::from("NULL")),
            Value::String(_) if text.parse::<i64>().is_ok() => Some(Value::from(text)),
            _ => None,
        },
        "float4" | "float8" | "numeric" | "money" => match value {
            Value::Int(number) => Some(Value::Int(*number)),
            Value::Float(number) => Some(Value::Float(*number)),
            Value::String(_) if text.is_empty() => Some(Value::from("NULL")),
            Value::String(_) if text.parse::<f64>().is_ok_and(f64::is_finite) => {
                Some(Value::from(text))
            }
            _ => None,
        },
        "bool" => match value {
            Value::Bool(true) => Some(Value::from("'t'")),
            Value::Bool(false) => Some(Value::from("'f'")),
            _ => match text.to_ascii_lowercase().as_str() {
                "t" | "true" | "y" | "yes" | "on" | "1" => Some(Value::from("'t'")),
                "f" | "false" | "n" | "no" | "off" | "0" => Some(Value::from("'f'")),
                _ => None,
            },
        },
        _ => Some(Value::from(escape::literal(&text))),
    };
    Ok(converted)
}
