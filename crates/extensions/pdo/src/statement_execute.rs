//! Executing bound parameters releases all statement borrows before awaiting I/O.

use crate::errors::PdoError;

use crate::drivers::{Execution, Prepared, Rows};
use crate::dsn::DriverName;
use crate::errors::{DbError, ErrorState};
use crate::parameters::{self, Bindings};
use crate::placeholders::Parameter;
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
        if matches!(state.borrow().prepared, Prepared::Sqlite(_)) {
            return crate::drivers::sqlite::statement::execute(context, &state, &arguments, method)
                .await;
        }
        if let Some(params) = arguments.first().filter(|value| !value.is_null()) {
            let mut statement = state.borrow_mut();
            let bindings = match (&statement.prepared, statement.sql.parameters.first()) {
                (Prepared::Emulated, Some(Parameter::Position(_))) => Bindings::packed(params),
                _ => None,
            };
            statement.bindings = match bindings {
                Some(bindings) => bindings,
                None => Bindings::from_execute(params)?,
            };
        }
        let (connection, sql, prepared, values) = {
            let statement = state.borrow();
            let values = match &statement.bindings {
                Bindings::Packed(array) => {
                    if !array.is_empty() && array.len() != statement.sql.parameters.len() {
                        Err(DbError::parameter(
                            "number of bound variables does not match number of tokens",
                        ))
                    } else {
                        Ok(parameters::read_packed(context, array)?)
                    }
                }
                Bindings::Keyed(bindings) => {
                    let ordered = match statement.prepared {
                        Prepared::Emulated if bindings.is_empty() => Ok(Vec::new()),
                        Prepared::Emulated => parameters::ordered(&statement.sql, bindings),
                        _ => crate::native_parameters::ordered(
                            &statement.sql,
                            bindings,
                            statement.connection.name,
                        ),
                    };
                    let omit = statement.connection.name == DriverName::Postgres
                        && !matches!(statement.prepared, Prepared::Emulated)
                        && statement.sql.parameters.is_empty();
                    match ordered {
                        Ok(bindings) => Ok(bindings
                            .into_iter()
                            .map(|binding| {
                                if omit {
                                    Ok(parameters::BoundValue::Null)
                                } else {
                                    binding.read(context)
                                }
                            })
                            .collect::<Result<Vec<_>, _>>()?),
                        Err(error) => Err(error),
                    }
                }
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
        let stream_owner = state.borrow_mut().stream_owner.take();
        let execution = {
            let attrs = connection.attributes.borrow();
            let mysql_stream = (connection.name == DriverName::Mysql && !attrs.mysql_buffered)
                .then(|| stream_owner.clone().unwrap_or_default());
            let postgres_stream = state
                .borrow()
                .postgres_lazy
                .then(|| stream_owner.clone().unwrap_or_default());
            Execution {
                national_strings: attrs.national_strings,
                mysql_stream,
                postgres_stream,
            }
        };
        let new_owner = execution
            .mysql_stream
            .clone()
            .or_else(|| execution.postgres_stream.clone());
        let driver = &connection.driver;
        if let Some(owner) = stream_owner
            && let Err(error) = driver.close_stream(owner).await
        {
            return fail(&state, context, error, method);
        }
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
        let result = driver.execute(&sql, &prepared, &values, execution).await;
        if let Some(active) = driver.observed_transaction_state() {
            connection.transaction.set(active);
        }
        match result {
            Ok(rows) => {
                let mut statement = state.borrow_mut();
                if !rows.columns.is_empty() {
                    statement.stream_owner = new_owner;
                }
                statement.set_rows(rows);
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
    mut error: DbError,
    method: &str,
) -> NativeResult {
    let mode = {
        let mut statement = state.borrow_mut();
        if statement.connection.name == crate::dsn::DriverName::Sqlite {
            crate::drivers::sqlite::normalize(
                &mut error,
                statement.connection.attributes.borrow().extended,
            );
        }
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
        statement.set_rows(Rows::default());
        statement.connection.attributes.borrow().errors
    };
    error.report(context, mode, method)
}
