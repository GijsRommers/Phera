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
        return crate::stream_fetch::fill(context, &state, all).await;
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
                        .map(crate::drivers::sqlite::statement::value)
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
        let (mode, row_class) = {
            let state = state(this.as_ref())?;
            let statement = state.borrow();
            (statement.fetch, statement.row_class.clone())
        };
        if matches!(
            crate::fetch::FetchMode::parse(
                phera_extension_api::args::int(&args, 0).unwrap_or(0),
                mode
            )?,
            crate::fetch::FetchMode::Class
        ) {
            let row_class = row_class.ok_or_else(|| {
                crate::errors::exception(
                    "SQLSTATE[HY000]: General error: No fetch class specified",
                    None,
                    0,
                )
            })?;
            return row_class.next(context, this.as_ref()).await;
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
        crate::fetch_class::RowClass::object(context, this.as_ref(), &args).await
    })
}

pub fn all<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        fill(context, this.as_ref(), true).await?;
        if let Some(class) =
            crate::fetch_class::RowClass::configured(context, &args, 0, "PDOStatement::fetchAll")
                .await?
        {
            return class.all(context, this.as_ref()).await;
        }
        if phera_extension_api::args::int(&args, 0).unwrap_or(0) == 0 {
            let class = {
                let state = state(this.as_ref())?;
                let statement = state.borrow();
                match statement.fetch {
                    crate::fetch::FetchMode::Class => statement.row_class.clone(),
                    _ => None,
                }
            };
            if let Some(class) = class {
                return class.all(context, this.as_ref()).await;
            }
        }
        crate::fetch_all::all(context, this.as_ref(), &args)
    })
}

pub fn set_mode<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move { crate::fetch::set_mode(context, this.as_ref(), &arguments).await })
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
        crate::stream_fetch::close(context, &state).await?;
        crate::statement::close(context, this.as_ref(), &args)
    })
}
