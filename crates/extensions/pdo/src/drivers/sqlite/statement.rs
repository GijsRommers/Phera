//! SQLite owns parameter numbering: `prepare('SELECT ?2, :name, @other')`.

use crate::drivers::{Column, Prepared, RowBuffer, Rows};
use crate::errors::{DbError, ErrorState};
use crate::parameters::Bindings;
use crate::statement::Statement;
use phera_extension_api::{ArrayKey, Context, NativeResult, PhpArray, Value};
use phera_sqlite::{Cell, kind};

use crate::constants::{PARAM_INT, PARAM_NULL, PARAM_STR};
use std::cell::RefCell;

pub async fn execute(
    context: &mut dyn Context,
    state: &RefCell<Statement>,
    arguments: &[Value],
    method: &str,
) -> NativeResult {
    if let Some(params) = arguments.first().filter(|v| !v.is_null()) {
        state.borrow_mut().bindings = Bindings::from_execute(params)?;
    }
    let (handle, bindings, extended) = {
        let statement = state.borrow();
        let Prepared::Sqlite(handle) = &statement.prepared else {
            return Err(phera_extension_api::NativeError::error(
                "this statement is not a SQLite statement",
            ));
        };
        let mut bindings = Vec::new();
        let Bindings::Keyed(parameters) = &statement.bindings else {
            return Err(phera_extension_api::NativeError::error(
                "SQLite statement bindings must be keyed",
            ));
        };
        for (key, binding) in parameters {
            let index = key.parse::<usize>().ok().map(|i| i + 1).or_else(|| {
                handle
                    .description
                    .parameters
                    .iter()
                    .position(|name| name.as_ref().is_some_and(|name| name == &format!(":{key}")))
                    .map(|i| i + 1)
            });
            let Some(index) = index.filter(|i| *i <= handle.description.parameters.len()) else {
                drop(statement);
                return crate::statement_execute::fail(
                    state,
                    context,
                    DbError {
                        state: "HY000".into(),
                        code: Some(25),
                        detail: "column index out of range".into(),
                    },
                    method,
                );
            };
            bindings.push((
                index,
                crate::drivers::sqlite::bindings::read(context, binding)?,
            ));
        }
        (
            handle.clone(),
            bindings,
            statement.connection.attributes.borrow().extended,
        )
    };
    match handle.execute(bindings).await {
        Ok(execution) => {
            let mut statement = state.borrow_mut();
            let columns: Vec<_> = execution
                .description
                .columns
                .into_iter()
                .enumerate()
                .map(|(index, column)| Column {
                    name: column.name.clone(),
                    metadata: crate::metadata::Metadata::Sqlite(
                        column,
                        execution
                            .first
                            .as_ref()
                            .and_then(|row| row.get(index))
                            .map_or(kind::NULL, Cell::kind),
                    ),
                })
                .collect();
            let affected = if execution.first.is_some() {
                statement.rows.affected
            } else {
                execution.changes
            };
            let mut values = RowBuffer::with_capacity(columns.len(), 1);
            if let Some(row) = execution.first {
                values.push(row.into_iter().map(value));
            }
            statement.set_rows(Rows {
                columns,
                values,
                affected,
                has_pending_rows: false,
            });
            statement.sqlite_executed = true;
            statement.error = ErrorState::Success;
            Ok(Value::Bool(true))
        }
        Err(error) => {
            let code = error.code;
            let mut error = crate::drivers::sqlite::error(error);
            if extended {
                error.code = Some(i64::from(code));
            }
            crate::statement_execute::fail(state, context, error, method)
        }
    }
}

pub fn value(cell: Cell) -> Value {
    match cell {
        Cell::Null => Value::Null,
        Cell::Integer(value) => Value::Int(value),
        Cell::Real(value) => Value::Float(value),
        Cell::Text(value) | Cell::Blob(value) => Value::from(value),
    }
}

/// `getColumnMeta()` for SQLite; `type_code` is the type of the value in
/// the current row, as pdo_sqlite reports it.
pub fn metadata(column: &phera_sqlite::Column, type_code: i64) -> PhpArray {
    let mut result = PhpArray::new();
    let (native, pdo) = match type_code {
        kind::INTEGER => ("integer", PARAM_INT),
        kind::FLOAT => ("double", PARAM_STR),
        kind::TEXT | kind::BLOB => ("string", PARAM_STR),
        _ => ("null", PARAM_NULL),
    };
    for (key, value) in [
        ("native_type", Value::from(native)),
        ("pdo_type", Value::Int(pdo)),
    ] {
        result.insert(ArrayKey::str(key), value);
    }
    if let Some(declared) = &column.declared {
        result.insert(
            ArrayKey::str("sqlite:decl_type"),
            Value::from(declared.clone()),
        );
    }
    if let Some(table) = &column.table {
        result.insert(ArrayKey::str("table"), Value::from(table.clone()));
    }
    let mut flags = PhpArray::new();
    if type_code == kind::BLOB {
        flags.insert(ArrayKey::Int(0), Value::from("blob"));
    }
    for (key, value) in [
        ("flags", Value::from(flags)),
        ("name", Value::from(column.name.clone())),
        ("len", Value::Int(-1)),
        ("precision", Value::Int(0)),
    ] {
        result.insert(ArrayKey::str(key), value);
    }
    result
}
