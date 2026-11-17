//! `fetch(PDO::FETCH_ASSOC)` returns named columns; `fetchAll(PDO::FETCH_KEY_PAIR)` builds a map.

use std::rc::Rc;

use phera_extension_api::value::to_php_string;
use phera_extension_api::{
    ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, Value, args,
};

use crate::attributes::{Attributes, ColumnCase};
use crate::drivers::Column;
use crate::errors::unsupported;
use crate::statement::state;

#[derive(Clone, Copy)]
pub enum FetchMode {
    Assoc,
    Num,
    Both,
    Object,
    Column,
    KeyPair,
}

impl FetchMode {
    pub fn parse(number: i64, default: Self) -> Result<Self, NativeError> {
        match number {
            0 => Ok(default),
            2 => Ok(Self::Assoc),
            3 => Ok(Self::Num),
            4 => Ok(Self::Both),
            5 => Ok(Self::Object),
            7 => Ok(Self::Column),
            12 => Ok(Self::KeyPair),
            _ => Err(unsupported("This PDO fetch mode")),
        }
    }
    pub fn number(self) -> i64 {
        match self {
            Self::Assoc => 2,
            Self::Num => 3,
            Self::Both => 4,
            Self::Object => 5,
            Self::Column => 7,
            Self::KeyPair => 12,
        }
    }
}

pub fn convert(
    context: &mut dyn Context,
    row: &[Value],
    columns: &[Column],
    mode: FetchMode,
    column: usize,
    attributes: &Attributes,
) -> NativeResult {
    if matches!(mode, FetchMode::Column) {
        return row
            .get(column)
            .cloned()
            .ok_or_else(|| NativeError::value_error("Invalid column index"))
            .and_then(|value| stringify(context, value, attributes));
    }
    if matches!(mode, FetchMode::KeyPair) {
        return Err(NativeError::value_error(
            "PDO::FETCH_KEY_PAIR is only valid for fetchAll()",
        ));
    }
    let mut values = PhpArray::new();
    for (index, (value, column)) in row.iter().zip(columns).enumerate() {
        let value = stringify(context, value.clone(), attributes)?;
        if !matches!(mode, FetchMode::Num) {
            let name = match attributes.case {
                ColumnCase::Natural => column.name.clone(),
                ColumnCase::Upper => column.name.to_uppercase(),
                ColumnCase::Lower => column.name.to_lowercase(),
            };
            let key = if matches!(mode, FetchMode::Object) {
                ArrayKey::str(name)
            } else {
                ArrayKey::from_value(&Value::from(name))?
            };
            values.insert(key, value.clone());
        }
        if matches!(mode, FetchMode::Num | FetchMode::Both) {
            values.insert(ArrayKey::Int(index as i64), value);
        }
    }
    if matches!(mode, FetchMode::Object) {
        context.create_object("stdClass", values)
    } else {
        Ok(Value::from(values))
    }
}

fn stringify(context: &mut dyn Context, value: Value, attributes: &Attributes) -> NativeResult {
    if !attributes.stringify || value.is_null() {
        return Ok(value);
    }
    if let Value::Bool(value) = value {
        return Ok(Value::from(if value { "1" } else { "0" }));
    }
    Ok(Value::String(to_php_string(&value, context.diagnostics())?))
}

pub fn fetch(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    if args::int(arguments, 1).unwrap_or(0) != 0 || args::int(arguments, 2).unwrap_or(0) != 0 {
        return Err(unsupported("Scrollable cursors"));
    }
    let state = state(this)?;
    let mut statement = state.borrow_mut();
    let mode = FetchMode::parse(args::int(arguments, 0).unwrap_or(0), statement.fetch)?;
    let Some(row) = statement.rows.values.get(statement.cursor).cloned() else {
        return Ok(Value::Bool(false));
    };
    statement.cursor += 1;
    let attributes = statement.connection.attributes.borrow().clone();
    convert(
        context,
        &row,
        &statement.rows.columns,
        mode,
        statement.column,
        &attributes,
    )
}

pub fn column(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let column = usize::try_from(args::int(arguments, 0).unwrap_or(0))
        .map_err(|_| NativeError::value_error("Column index must be greater than or equal to 0"))?;
    let state = state(this)?;
    let mut statement = state.borrow_mut();
    let Some(row) = statement.rows.values.get(statement.cursor).cloned() else {
        return Ok(Value::Bool(false));
    };
    statement.cursor += 1;
    let attributes = statement.connection.attributes.borrow().clone();
    convert(
        context,
        &row,
        &statement.rows.columns,
        FetchMode::Column,
        column,
        &attributes,
    )
}

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
    let number = args::int(arguments, 0).unwrap_or(0);
    let grouping = match number & 0x60 {
        0 => Grouping::None,
        32 => Grouping::Group,
        64 => Grouping::Unique,
        _ => return Err(unsupported("This PDO fetch mode")),
    };
    let mode = FetchMode::parse(number & !0x60, statement.fetch)?;
    let column = usize::try_from(args::int(arguments, 1).unwrap_or(statement.column as i64))
        .map_err(|_| NativeError::value_error("Invalid column index"))?;
    let attributes = statement.connection.attributes.borrow().clone();
    let mut result = PhpArray::new();
    for row in &statement.rows.values[statement.cursor..] {
        match grouping {
            Grouping::None if matches!(mode, FetchMode::KeyPair) => {
                if row.len() != 2 {
                    return Err(NativeError::throw(
                        "PDOException",
                        "SQLSTATE[HY000]: General error: PDO::FETCH_KEY_PAIR fetch mode \
                         requires the result set to contain exactly 2 columns.",
                    ));
                }
                let key = stringify(context, row[0].clone(), &attributes)?;
                result.insert(
                    ArrayKey::from_value(&key)?,
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
                let key = ArrayKey::from_value(first)?;
                let value = convert(
                    context,
                    &row[1..],
                    &statement.rows.columns[1..],
                    mode,
                    column,
                    &attributes,
                )?;
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

pub fn set_mode(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let state = state(this)?;
    let mut statement = state.borrow_mut();
    statement.fetch = FetchMode::parse(args::int(arguments, 0).unwrap_or(0), statement.fetch)?;
    statement.column = usize::try_from(args::int(arguments, 1).unwrap_or(0))
        .map_err(|_| NativeError::value_error("Invalid column index"))?;
    Ok(Value::Bool(true))
}

/// `fetchObject()` constructs stdClass; user-defined constructors need VM callback support.
pub fn object(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    if args::string(arguments, 0).is_some_and(|name| name.to_string_lossy() != "stdClass")
        || args::array(arguments, 1).is_some_and(|args| !args.is_empty())
    {
        return Err(unsupported("Fetching into a custom class"));
    }
    fetch(context, this, &[Value::Int(5)])
}
