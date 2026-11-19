//! `mysqli_query($db, "SELECT 1")` returns a result, and DML returns true.

use phera_db_drivers::drivers::{RawRows, mysql::Outcome};
use phera_db_drivers::errors::DbError;
use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use super::{Connection, Handle, run};
use crate::{errors, result};

pub const QUERY: usize = 0;
pub const REAL_QUERY: usize = 1;
pub const MULTI_QUERY: usize = 2;
pub const NEXT: usize = 3;
pub const STORE: usize = 4;
pub const USE: usize = 5;
pub const MORE: usize = 6;

pub async fn execute<const OP: usize>(
    context: &mut dyn Context,
    state: Handle,
    arguments: &[Value],
    call: &str,
) -> NativeResult {
    let lease = state.borrow().lease()?;
    if OP == NEXT {
        let mut state = state.borrow_mut();
        state.pending = None;
        return match state.more.pop_front() {
            Some(Ok(outcome)) => {
                state.observe(&outcome);
                state.pending = outcome.rows.has_columns().then_some(outcome);
                Ok(Value::Bool(true))
            }
            Some(Err(error)) => failure(context, &mut state, error, call),
            None => Ok(Value::Bool(false)),
        };
    }
    let sql = args::bytes(arguments, 0).to_vec();
    if sql.is_empty() {
        let index = if call.starts_with("mysqli::") { 1 } else { 2 };
        return Err(NativeError::value_error(format!(
            "{call}(): Argument #{index} ($query) must not be empty"
        )));
    }
    let mode = args::int(arguments, 1).unwrap_or_default();
    if OP == QUERY {
        if mode & 8 != 0 {
            return Err(NativeError::error("MYSQLI_ASYNC is not supported"));
        }
        if !matches!(mode, 0 | 1 | 16) {
            let index = if call.starts_with("mysqli::") { 2 } else { 3 };
            return Err(NativeError::value_error(format!(
                "{call}(): Argument #{index} ($result_mode) must be either MYSQLI_USE_RESULT                  or MYSQLI_STORE_RESULT"
            )));
        }
    }
    if state.borrow().pending.is_some() || !state.borrow().more.is_empty() {
        return failure(
            context,
            &mut state.borrow_mut(),
            DbError {
                state: "HY000".into(),
                code: Some(2014),
                detail: "Commands out of sync; you can't run this command now".into(),
            },
            call,
        );
    }
    let output = run(&lease, move |driver| {
        Box::pin(async move {
            let outcomes = driver.query_native(&sql, OP == MULTI_QUERY).await?;
            Ok((outcomes, driver.backslashes_enabled()))
        })
    })
    .await;
    let (outcomes, backslashes) = match output {
        Ok(output) => output,
        Err(error) => return failure(context, &mut state.borrow_mut(), error, call),
    };
    let mut outcomes = outcomes.into_iter();
    let Some(Ok(outcome)) = outcomes.next() else {
        return Ok(Value::Bool(false));
    };
    let mut state = state.borrow_mut();
    state.observe(&outcome);
    state.backslashes = backslashes;
    if OP == QUERY {
        if outcome.rows.has_columns() {
            return result::create(context, outcome.rows, state.native_numbers, mode == 1);
        }
    } else {
        state.more = outcomes.collect();
        if outcome.rows.has_columns() {
            state.pending = Some(outcome);
        }
    }
    Ok(Value::Bool(true))
}

pub fn result_operation<const OP: usize>(context: &mut dyn Context, state: Handle) -> NativeResult {
    let mut state = state.borrow_mut();
    state.lease()?;
    if OP == MORE {
        return Ok(Value::Bool(!state.more.is_empty()));
    }
    match state.pending.take() {
        Some(outcome) if outcome.rows.has_columns() => {
            result::create(context, outcome.rows, state.native_numbers, OP == USE)
        }
        _ => Ok(Value::Bool(false)),
    }
}

impl Connection {
    pub fn observe(&mut self, result: &Outcome) {
        self.error = None;
        self.field_count = match &result.rows {
            RawRows::Mysql { columns, .. } => columns.len(),
            _ => 0,
        };
        self.reply.affected = result.reply.affected;
        self.reply.insert_id = result.reply.insert_id;
        self.reply.warnings = result.reply.warnings;
        self.reply.info.clone_from(&result.reply.info);
    }
}

pub fn failure(
    context: &mut dyn Context,
    state: &mut Connection,
    error: DbError,
    call: &str,
) -> NativeResult {
    state.error = Some(error.clone());
    state.reply.affected = 0;
    errors::report(context, &error, call, false)
}
