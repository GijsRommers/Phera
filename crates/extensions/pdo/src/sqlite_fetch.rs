//! `fetch()` awaits the next SQLite row; MySQL and PostgreSQL retain their existing paths.

use crate::drivers::Prepared;
use crate::statement::state;
use phera_extension_api::{Context, LocalFuture, NativeResult, Object, Value};
use std::rc::Rc;

async fn fill(context: &mut dyn Context, this: Option<&Rc<Object>>, all: bool) -> NativeResult {
    let state = state(this)?;
    let handle = match &state.borrow().prepared {
        Prepared::Sqlite(handle) => Some(handle.clone()),
        _ => None,
    };
    let Some(handle) = handle else {
        return Ok(Value::Null);
    };
    loop {
        if !all && state.borrow().cursor < state.borrow().rows.values.len() {
            break;
        }
        match handle.fetch().await {
            Ok(Some(row)) => {
                let mut statement = state.borrow_mut();
                for (column, cell) in statement.rows.columns.iter_mut().zip(&row) {
                    if let crate::metadata::Metadata::Sqlite(_, kind) = &mut column.metadata {
                        *kind = cell.kind();
                    }
                }
                statement.rows.values.push(
                    row.into_iter()
                        .map(crate::sqlite_statement::value)
                        .collect(),
                );
            }
            Ok(None) => {
                for column in &mut state.borrow_mut().rows.columns {
                    if let crate::metadata::Metadata::Sqlite(_, kind) = &mut column.metadata {
                        *kind = 5;
                    }
                }
                break;
            }
            Err(error) => {
                return crate::statement_execute::fail(
                    &state,
                    context,
                    crate::drivers::sqlite::error(error),
                    "PDOStatement::fetch",
                );
            }
        }
    }
    Ok(Value::Null)
}

pub fn fetch<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        if matches!(
            fill(context, this.as_ref(), false).await?,
            Value::Bool(false)
        ) {
            return Ok(Value::Bool(false));
        }
        crate::fetch::fetch(context, this.as_ref(), &args)
    })
}

pub fn column<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        fill(context, this.as_ref(), false).await?;
        crate::fetch::column(context, this.as_ref(), &args)
    })
}

pub fn object<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        fill(context, this.as_ref(), false).await?;
        crate::fetch::object(context, this.as_ref(), &args)
    })
}

pub fn all<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        fill(context, this.as_ref(), true).await?;
        crate::fetch_all::all(context, this.as_ref(), &args)
    })
}

pub fn close<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = state(this.as_ref())?;
        let handle = match &state.borrow().prepared {
            Prepared::Sqlite(handle) => Some(handle.clone()),
            _ => None,
        };
        if let Some(handle) = handle {
            handle.reset().await.map_err(|error| {
                crate::errors::exception(error.message, None, i64::from(error.code))
            })?;
        }
        crate::statement::close(context, this.as_ref(), &args)
    })
}
