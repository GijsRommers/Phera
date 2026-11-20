//! Grouped, unique, key-pair and ordinary fetchAll result construction.

use crate::constants::FETCH_GROUP;
use crate::fetch::{FetchMode, ValueOptions, convert, stringify};
use crate::statement::{Statement, state};
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
        FETCH_GROUP => Grouping::Group,
        _ => Grouping::Unique,
    };
    let mode = configuration.mode;
    let column = configuration.column;
    let Statement {
        connection,
        rows,
        keys,
        cursor,
        fetch_flags,
        ..
    } = &mut *statement;
    let attributes = ValueOptions::from(&*connection.attributes.borrow());
    let keys = if mode.uses_names() {
        keys.get(&rows.columns)
    } else {
        &[]
    };
    let mut result = PhpArray::with_capacity(rows.values.len().saturating_sub(*cursor));
    while let Some(row) = rows.values.get_mut(*cursor) {
        *cursor += 1;
        match grouping {
            Grouping::None if matches!(mode, FetchMode::KeyPair) => {
                let [first, second] = row else {
                    return Err(crate::errors::exception(
                        "SQLSTATE[HY000]: General error: PDO::FETCH_KEY_PAIR fetch mode \
                         requires the result set to contain exactly 2 columns.",
                        None,
                        0,
                    ));
                };
                let key = stringify(context, std::mem::take(first), attributes)?;
                result.insert(
                    group_key(context, &key)?,
                    stringify(context, std::mem::take(second), attributes)?,
                );
            }
            Grouping::None => {
                result.push(convert(
                    context,
                    row.iter_mut().map(std::mem::take),
                    keys,
                    mode,
                    column,
                    attributes,
                )?)?;
            }
            Grouping::Group | Grouping::Unique => {
                let Some((first, rest)) = row.split_first_mut() else {
                    continue;
                };
                let (key, value) = if matches!(mode, FetchMode::Column) {
                    let explicit = arguments.len() > 1
                        || (*fetch_flags != 0
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
                        .cloned()
                        .ok_or_else(|| NativeError::value_error("Invalid column index"))?;
                    let value = convert(
                        context,
                        row.iter_mut().map(std::mem::take),
                        keys,
                        mode,
                        value_column,
                        attributes,
                    )?;
                    let key = stringify(context, key, attributes)?;
                    (group_key(context, &key)?, value)
                } else {
                    let first = stringify(context, std::mem::take(first), attributes)?;
                    let key = group_key(context, &first)?;
                    let value = convert(
                        context,
                        rest.iter_mut().map(std::mem::take),
                        keys.get(1..).unwrap_or_default(),
                        mode,
                        column,
                        attributes,
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
    Ok(Value::from(result))
}

fn group_key(context: &mut dyn Context, value: &Value) -> Result<ArrayKey, NativeError> {
    let text = to_php_string(value, context.diagnostics())?;
    Ok(ArrayKey::from_value(&Value::String(text))?)
}
