//! `fetch(PDO::FETCH_ASSOC)` returns named columns; `fetchAll(PDO::FETCH_KEY_PAIR)` builds a map.

use std::rc::Rc;

use phera_extension_api::value::to_php_string;
use phera_extension_api::{
    ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, Value, args,
};

use crate::attributes::{Attributes, NullHandling};
use crate::column_keys::Layout;
use crate::constants::{
    FETCH_ASSOC, FETCH_BOTH, FETCH_CLASS, FETCH_COLUMN, FETCH_DEFAULT, FETCH_KEY_PAIR, FETCH_NAMED,
    FETCH_NUM, FETCH_OBJ,
};
use crate::errors::unsupported;
use crate::statement::{Statement, state};

#[derive(Clone, Copy)]
pub struct ValueOptions {
    stringify: bool,
    nulls: NullHandling,
}

impl From<&Attributes> for ValueOptions {
    fn from(attributes: &Attributes) -> Self {
        Self {
            stringify: attributes.stringify,
            nulls: attributes.oracle_nulls,
        }
    }
}

#[derive(Clone, Copy)]
pub enum FetchMode {
    Assoc,
    Num,
    Both,
    Object,
    Class,
    Column,
    KeyPair,
    Named,
    Other(i64),
}

impl FetchMode {
    pub fn parse(number: i64, default: Self) -> Result<Self, NativeError> {
        match number {
            FETCH_DEFAULT => Ok(default),
            FETCH_ASSOC => Ok(Self::Assoc),
            FETCH_NUM => Ok(Self::Num),
            FETCH_BOTH => Ok(Self::Both),
            FETCH_OBJ => Ok(Self::Object),
            FETCH_CLASS => Ok(Self::Class),
            FETCH_COLUMN => Ok(Self::Column),
            FETCH_KEY_PAIR => Ok(Self::KeyPair),
            FETCH_NAMED => Ok(Self::Named),
            _ => Err(unsupported("This PDO fetch mode")),
        }
    }
    pub fn number(self) -> i64 {
        match self {
            Self::Assoc => FETCH_ASSOC,
            Self::Num => FETCH_NUM,
            Self::Both => FETCH_BOTH,
            Self::Object => FETCH_OBJ,
            Self::Class => FETCH_CLASS,
            Self::Column => FETCH_COLUMN,
            Self::KeyPair => FETCH_KEY_PAIR,
            Self::Named => FETCH_NAMED,
            Self::Other(number) => number,
        }
    }

    pub fn default_attribute(number: i64) -> Self {
        Self::parse(number, Self::Other(number)).unwrap_or(Self::Other(number))
    }

    pub fn uses_names(self) -> bool {
        matches!(
            self,
            Self::Assoc | Self::Both | Self::Object | Self::Class | Self::Named
        )
    }
}

pub fn convert(
    context: &mut dyn Context,
    mut row: impl ExactSizeIterator<Item = Value>,
    layout: Layout<'_>,
    mode: FetchMode,
    column: usize,
    attributes: ValueOptions,
) -> NativeResult {
    if matches!(mode, FetchMode::Other(_)) {
        return Err(unsupported("This PDO fetch mode"));
    }
    if matches!(mode, FetchMode::Column) {
        return row
            .nth(column)
            .ok_or_else(|| NativeError::value_error("Invalid column index"))
            .and_then(|value| stringify(context, value, attributes));
    }
    if matches!(mode, FetchMode::KeyPair) {
        return Err(NativeError::value_error(
            "PDO::FETCH_KEY_PAIR is only valid for fetchAll()",
        ));
    }
    let keys = layout.keys;
    if layout.distinct
        && matches!(
            mode,
            FetchMode::Assoc | FetchMode::Named | FetchMode::Object
        )
    {
        let capacity = row.len().min(keys.len());
        let entries = row.zip(keys).map(|(value, key)| {
            let key = if matches!(mode, FetchMode::Object) {
                &key.property
            } else {
                &key.array
            };
            stringify(context, value, attributes).map(|value| (key.clone(), value))
        });
        let array = PhpArray::try_from_distinct_with_capacity(entries, capacity)?;
        return if matches!(mode, FetchMode::Object) {
            context.create_object("stdClass", array)
        } else {
            Ok(Value::from(array))
        };
    }
    let mut values = PhpArray::with_capacity(row.len());
    if matches!(mode, FetchMode::Num) {
        for value in row {
            values.push(stringify(context, value, attributes)?)?;
        }
    } else if matches!(mode, FetchMode::Named) {
        let mut duplicates = std::collections::HashSet::new();
        for (value, key) in row.zip(keys) {
            let value = stringify(context, value, attributes)?;
            let key = &key.array;
            if let Some(previous) = values.get(key) {
                let mut group = if duplicates.insert(key.clone()) {
                    let mut group = PhpArray::new();
                    group.push(previous.clone())?;
                    group
                } else if let Value::Array(group) = previous {
                    (**group).clone()
                } else {
                    PhpArray::new()
                };
                group.push(value)?;
                values.insert(key.clone(), Value::from(group));
            } else {
                values.insert(key.clone(), value);
            }
        }
    } else {
        for (index, (value, key)) in row.zip(keys).enumerate() {
            let value = stringify(context, value, attributes)?;
            match mode {
                FetchMode::Object => values.insert(key.property.clone(), value),
                FetchMode::Both => {
                    values.insert(key.array.clone(), value.clone());
                    values.insert(ArrayKey::Int(index as i64), value)
                }
                _ => values.insert(key.array.clone(), value),
            };
        }
    }
    if matches!(mode, FetchMode::Object) {
        context.create_object("stdClass", values)
    } else {
        Ok(Value::from(values))
    }
}

pub(crate) fn stringify(
    context: &mut dyn Context,
    value: Value,
    attributes: ValueOptions,
) -> NativeResult {
    if matches!(attributes.nulls, NullHandling::EmptyString)
        && value.as_str_bytes().is_some_and(<[u8]>::is_empty)
    {
        return Ok(Value::Null);
    }
    if value.is_null() && matches!(attributes.nulls, NullHandling::ToString) {
        return Ok(Value::from(""));
    }
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
    let column = statement.column;
    next(context, &mut statement, mode, column)
}

pub fn column(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let state = state(this)?;
    let mut statement = state.borrow_mut();
    if statement.cursor >= statement.rows.values.len() {
        return Ok(Value::Bool(false));
    }
    let Ok(column) = usize::try_from(args::int(arguments, 0).unwrap_or(0)) else {
        statement.cursor += 1;
        return Err(NativeError::value_error(
            "Column index must be greater than or equal to 0",
        ));
    };
    next(context, &mut statement, FetchMode::Column, column)
}

fn next(
    context: &mut dyn Context,
    statement: &mut Statement,
    mode: FetchMode,
    column: usize,
) -> NativeResult {
    let Statement {
        rows,
        keys,
        cursor,
        connection,
        ..
    } = statement;
    let Some(row) = rows.values.get_mut(*cursor) else {
        return Ok(Value::Bool(false));
    };
    *cursor += 1;
    let attributes = ValueOptions::from(&*connection.attributes.borrow());
    let keys = if mode.uses_names() {
        keys.get(&rows.columns)
    } else {
        Layout::default()
    };
    convert(
        context,
        row.iter_mut().map(std::mem::take),
        keys,
        mode,
        column,
        attributes,
    )
}

pub async fn set_mode(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let state = state(this)?;
    let (mode, column, flags) = {
        let statement = state.borrow();
        (statement.fetch, statement.column, statement.fetch_flags)
    };
    let configuration = crate::fetch_options::validate(
        crate::fetch_options::Method::SetMode,
        arguments,
        mode,
        column,
        flags,
    )?;
    let class = crate::fetch_class::RowClass::configured(
        context,
        arguments,
        0,
        "PDOStatement::setFetchMode",
    )
    .await?;
    let mut statement = state.borrow_mut();
    statement.fetch = configuration.mode;
    statement.column = configuration.column;
    statement.fetch_flags = configuration.flags;
    if let Some(class) = class {
        statement.row_class = Some(class);
    }
    Ok(Value::Bool(true))
}
