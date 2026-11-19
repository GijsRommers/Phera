//! Fetching PDO rows that are still on the server, without keeping rows already returned to PHP.

use std::cell::RefCell;

use phera_extension_api::{Context, NativeResult, Value};

use crate::statement::Statement;
use crate::statement_execute::fail;

pub async fn fill(
    context: &mut dyn Context,
    state: &RefCell<Statement>,
    all: bool,
) -> NativeResult {
    let (connection, owner) = {
        let statement = state.borrow();
        let Some(owner) = statement.stream_owner.clone() else {
            return Ok(Value::Null);
        };
        (statement.connection.clone(), owner)
    };
    let result = {
        let mut lease = connection.driver.lock().await;
        let result = lease.read_mysql_stream(owner, all).await;
        if let Some(active) = lease.observed_transaction_state() {
            connection.transaction.set(active);
        }
        result
    };
    let mut statement = state.borrow_mut();
    statement.rows.values.clear();
    statement.cursor = 0;
    match result {
        Ok(Some((rows, done))) => {
            statement.rows.values = rows.values;
            if done {
                statement.stream_owner = None;
            }
        }
        Ok(None) => {
            statement.stream_owner = None;
        }
        Err(error) => {
            statement.stream_owner = None;
            drop(statement);
            return fail(state, context, error, "PDOStatement::fetch");
        }
    }
    Ok(Value::Null)
}

pub async fn close(context: &mut dyn Context, state: &RefCell<Statement>) -> NativeResult {
    let (connection, owner) = {
        let mut statement = state.borrow_mut();
        let Some(owner) = statement.stream_owner.take() else {
            return Ok(Value::Null);
        };
        (statement.connection.clone(), owner)
    };
    let result = connection
        .driver
        .lock()
        .await
        .close_mysql_stream(owner)
        .await;
    match result {
        Ok(()) => Ok(Value::Null),
        Err(error) => fail(state, context, error, "PDOStatement::closeCursor"),
    }
}
