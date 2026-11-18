//! Executing bound parameters releases all statement borrows before awaiting I/O.

use crate::drivers::{Prepared, Rows};
use crate::errors::{DbError, ErrorState};
use crate::parameters;
use crate::statement::{Statement, state};
use phera_extension_api::{Context, LocalFuture, NativeResult, Object, Value};
use std::cell::RefCell;
use std::rc::Rc;

pub fn execute<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    execute_named(context, this, arguments, "PDOStatement::execute")
}

pub fn execute_named<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    method: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = state(this.as_ref())?;
        if let Some(params) = arguments.first().filter(|value| !value.is_null()) {
            state.borrow_mut().bindings = parameters::execute_bindings(params)?;
        }
        let (connection, sql, prepared, values) = {
            let statement = state.borrow();
            let ordered = match statement.prepared {
                Prepared::Emulated if statement.bindings.is_empty() => Ok(Vec::new()),
                Prepared::Emulated => parameters::ordered(&statement.sql, &statement.bindings),
                _ => crate::native_parameters::ordered(
                    &statement.sql,
                    &statement.bindings,
                    statement.connection.name,
                ),
            };
            let values = match ordered {
                Ok(bindings) => Ok(bindings
                    .into_iter()
                    .map(|binding| {
                        if statement.connection.name == crate::dsn::DriverName::Postgres
                            && !matches!(statement.prepared, Prepared::Emulated)
                            && statement.sql.parameters.is_empty()
                        {
                            Ok(parameters::BoundValue::Null)
                        } else {
                            binding.read(context)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?),
                Err(error) => Err(error),
            };
            (
                statement.connection.clone(),
                statement.sql.clone(),
                statement.prepared.clone(),
                values,
            )
        };
        let values = match values {
            Ok(values) => values,
            Err(error) => return fail(&state, context, error, method),
        };
        let mut driver = connection.driver.lock().await;
        let prepared = if let Prepared::PostgresDeferred { number } = prepared {
            match driver.prepare(&sql, number).await {
                Ok(prepared) => {
                    state.borrow_mut().prepared = prepared.clone();
                    prepared
                }
                Err(error) => return fail(&state, context, error, method),
            }
        } else {
            prepared
        };
        let result = driver.execute(&sql, &prepared, &values).await;
        if let Some(active) = driver.observed_transaction_state() {
            connection.transaction.set(active);
        }
        drop(driver);
        match result {
            Ok(rows) => {
                let mut statement = state.borrow_mut();
                statement.rows = rows;
                statement.cursor = 0;
                statement.error = ErrorState::Success;
                Ok(Value::Bool(true))
            }
            Err(error) => fail(&state, context, error, method),
        }
    })
}

pub fn fail(
    state: &RefCell<Statement>,
    context: &mut dyn Context,
    error: DbError,
    method: &str,
) -> NativeResult {
    let mode = {
        let mut statement = state.borrow_mut();
        let mut diagnostic = error.clone();
        if error.state == "HY093" {
            if statement.connection.name == crate::dsn::DriverName::Postgres {
                if let Some(previous) = statement.connection.last_error.borrow().as_ref() {
                    diagnostic.code = previous.code;
                    diagnostic.detail = previous.detail.clone();
                }
            } else if let ErrorState::Failed(previous) = &statement.error {
                diagnostic.code = previous.code;
                diagnostic.detail = previous.detail.clone();
            } else {
                diagnostic.code = None;
                diagnostic.detail.clear();
            }
        }
        statement.error = ErrorState::Failed(diagnostic);
        if error.state != "HY093" {
            *statement.connection.last_error.borrow_mut() = Some(error.clone());
        }
        statement.rows = Rows::default();
        statement.cursor = 0;
        statement.connection.attributes.borrow().errors
    };
    error.report(context, mode, method)
}
