//! Grouped, unique, key-pair and ordinary fetchAll result construction.

use crate::fetch::{FetchMode, convert, stringify};
use crate::statement::state;
use phera_extension_api::value::to_php_string;
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, Value};
use std::rc::Rc;

#[derive(Clone, Copy)]
enum Grouping {
    None,
    Group,
    Unique,
}

pub fn all(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let state = state(this)?;
    let mut statement = state.borrow_mut();
    let configuration = crate::fetch_options::validate(
        crate::fetch_options::Method::All,
        arguments,
        statement.fetch,
        statement.column,
        statement.fetch_flags,
    )?;
    let grouping = match configuration.flags {
        0 => Grouping::None,
        32 => Grouping::Group,
        _ => Grouping::Unique,
    };
    let mode = configuration.mode;
    let column = configuration.column;
    let attributes = statement.connection.attributes.borrow().clone();
    let mut result = PhpArray::new();
    for row in &statement.rows.values[statement.cursor..] {
        match grouping {
            Grouping::None if matches!(mode, FetchMode::KeyPair) => {
                if row.len() != 2 {
                    return Err(crate::errors::exception(
                        "SQLSTATE[HY000]: General error: PDO::FETCH_KEY_PAIR fetch mode \
                         requires the result set to contain exactly 2 columns.",
                        None,
                        0,
                    ));
                }
                let key = stringify(context, row[0].clone(), &attributes)?;
                result.insert(
                    group_key(context, &key)?,
                    stringify(context, row[1].clone(), &attributes)?,
                );
            }
            Grouping::None => {
                result.push(convert(
                    context,
                    row,
                    &statement.rows.columns,
                    mode,
                    column,
                    &attributes,
                )?)?;
            }
            Grouping::Group | Grouping::Unique => {
                let Some(first) = row.first() else {
                    continue;
                };
                let (key, value) = if matches!(mode, FetchMode::Column) {
                    let explicit = arguments.len() > 1
                        || (statement.fetch_flags != 0
                            && (arguments.is_empty()
                                || matches!(arguments.first(), Some(Value::Int(0)))));
                    let key_column = if matches!(grouping, Grouping::Group) && explicit {
                        column
                    } else {
                        0
                    };
                    let value_column = if explicit {
                        if matches!(grouping, Grouping::Group) {
                            0
                        } else {
                            column
                        }
                    } else {
                        1
                    };
                    let key = row
                        .get(key_column)
                        .ok_or_else(|| NativeError::value_error("Invalid column index"))?;
                    let value = convert(
                        context,
                        row,
                        &statement.rows.columns,
                        mode,
                        value_column,
                        &attributes,
                    )?;
                    (group_key(context, key)?, value)
                } else {
                    let key = group_key(context, first)?;
                    let value = convert(
                        context,
                        &row[1..],
                        &statement.rows.columns[1..],
                        mode,
                        column,
                        &attributes,
                    )?;
                    (key, value)
                };
                if matches!(grouping, Grouping::Unique) {
                    result.insert(key, value);
                } else {
                    let mut group = match result.get(&key) {
                        Some(Value::Array(values)) => (**values).clone(),
                        _ => PhpArray::new(),
                    };
                    group.push(value)?;
                    result.insert(key, Value::from(group));
                }
            }
        }
    }
    statement.cursor = statement.rows.values.len();
    Ok(Value::from(result))
}

fn group_key(context: &mut dyn Context, value: &Value) -> Result<ArrayKey, NativeError> {
    let text = to_php_string(value, context.diagnostics())?;
    Ok(ArrayKey::from_value(&Value::String(text))?)
}
