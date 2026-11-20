//! `fetch(PDO::FETCH_ASSOC)` returns named columns; `fetchAll(PDO::FETCH_KEY_PAIR)` builds a map.

use std::rc::Rc;

use phera_extension_api::value::to_php_string;
use phera_extension_api::{
    ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, Value, args,
};

use crate::attributes::{Attributes, ColumnCase, NullHandling};
use crate::constants::{
    FETCH_ASSOC, FETCH_BOTH, FETCH_CLASS, FETCH_COLUMN, FETCH_DEFAULT, FETCH_KEY_PAIR, FETCH_NAMED,
    FETCH_NUM, FETCH_OBJ,
};
use crate::drivers::Column;
use crate::errors::unsupported;
use crate::statement::state;

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
}

pub fn convert(
    context: &mut dyn Context,
    row: &[Value],
    columns: &[Column],
    mode: FetchMode,
    column: usize,
    attributes: &Attributes,
) -> NativeResult {
    if matches!(mode, FetchMode::Other(_)) {
        return Err(unsupported("This PDO fetch mode"));
    }
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
    let mut duplicates = std::collections::HashSet::new();
    for (index, (value, column)) in row.iter().zip(columns).enumerate() {
        let value = stringify(context, value.clone(), attributes)?;
        if !matches!(mode, FetchMode::Num) {
            let name = column_name(column, attributes);
            let key = if matches!(mode, FetchMode::Object) {
                ArrayKey::str(name)
            } else {
                ArrayKey::from_value(&Value::from(name))?
            };
            match (mode, values.get(&key)) {
                (FetchMode::Named, Some(previous)) => {
                    let mut group = if duplicates.insert(key.clone()) {
                        let mut group = PhpArray::new();
                        group.push(previous.clone())?;
                        group
                    } else if let Value::Array(group) = previous {
                        (**group).clone()
                    } else {
                        PhpArray::new()
                    };
                    group.push(value.clone())?;
                    values.insert(key, Value::from(group));
                }
                _ => {
                    values.insert(key, value.clone());
                }
            }
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

pub(crate) fn column_name(column: &Column, attributes: &Attributes) -> String {
    let name = match (&column.metadata, attributes.mysql_table_names) {
        (crate::metadata::Metadata::Mysql(mysql), true) => {
            format!("{}.{}", mysql.table_str(), column.name)
        }
        _ => column.name.clone(),
    };
    match attributes.case {
        ColumnCase::Natural => name,
        ColumnCase::Upper => name.to_ascii_uppercase(),
        ColumnCase::Lower => name.to_ascii_lowercase(),
    }
}

pub(crate) fn stringify(
    context: &mut dyn Context,
    value: Value,
    attributes: &Attributes,
) -> NativeResult {
    if matches!(attributes.oracle_nulls, NullHandling::EmptyString)
        && matches!(&value, Value::String(text) if text.as_bytes().is_empty())
    {
        return Ok(Value::Null);
    }
    if value.is_null() && matches!(attributes.oracle_nulls, NullHandling::ToString) {
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
    let state = state(this)?;
    let mut statement = state.borrow_mut();
    let Some(row) = statement.rows.values.get(statement.cursor).cloned() else {
        return Ok(Value::Bool(false));
    };
    statement.cursor += 1;
    let column = usize::try_from(args::int(arguments, 0).unwrap_or(0))
        .map_err(|_| NativeError::value_error("Column index must be greater than or equal to 0"))?;

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
