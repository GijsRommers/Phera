//! `pg_send_query($db, $sql)` starts asynchronous work; `pg_get_result($db)` collects it.

use std::rc::Rc;

use phera_db_drivers::drivers::postgres::{NativeResult as Rows, NativeSequence, Postgres};
use phera_db_drivers::errors::DbError;
use phera_db_drivers::parameters::BoundValue;
use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, PhpArray, Value, args, value::to_php_string,
};

use crate::{
    connection::{self, Handle},
    result,
};

pub const SIMPLE: usize = 0;
pub const PARAMS: usize = 1;
pub const PREPARE: usize = 2;
pub const EXECUTE: usize = 3;

enum Command {
    Simple(String),
    Params(String, Vec<BoundValue>),
    Prepare(String, String),
    Execute(String, Vec<BoundValue>),
}

impl Command {
    async fn run(self, driver: &mut Postgres) -> Result<NativeSequence, DbError> {
        let results = match self {
            Self::Simple(sql) => driver.native_query(&sql).await,
            Self::Params(sql, values) => driver.native_query_params(&sql, &values).await,
            Self::Prepare(name, sql) => driver.native_prepare(&name, &sql).await,
            Self::Execute(name, values) => driver.native_execute(&name, &values).await,
        };
        match results {
            Ok(results) => Ok(results),
            Err(error) => Ok(vec![Rows {
                error: Some(error),
                ..Rows::default()
            }]),
        }
    }

    fn parse<const OP: usize>(
        context: &mut dyn Context,
        arguments: &[Value],
        start: usize,
        call: &str,
    ) -> Result<Self, NativeError> {
        let source = text(
            arguments,
            start,
            call,
            if OP == EXECUTE || OP == PREPARE {
                "statement_name"
            } else {
                "query"
            },
        )?;
        let params = |context: &mut dyn Context| -> Result<Vec<BoundValue>, NativeError> {
            let values = args::array(arguments, start + 1)
                .map(Rc::as_ref)
                .ok_or_else(|| {
                    NativeError::type_error(format!(
                        "{call}(): Argument #{} ($params) must be of type array, {} given",
                        start + 2,
                        args::value(arguments, start + 1).deref().type_name()
                    ))
                })?;
            parameters(context, values)
        };
        match OP {
            SIMPLE => Ok(Self::Simple(source)),
            PARAMS => Ok(Self::Params(source, params(context)?)),
            PREPARE => Ok(Self::Prepare(
                source,
                text(arguments, start + 1, call, "query")?,
            )),
            _ => Ok(Self::Execute(source, params(context)?)),
        }
    }
}

fn parameters(
    context: &mut dyn Context,
    params: &PhpArray,
) -> Result<Vec<BoundValue>, NativeError> {
    params
        .iter()
        .map(|(_, value)| match &*value.deref() {
            Value::Null => Ok(BoundValue::Null),
            Value::Bool(true) => Ok(BoundValue::String(b"t".to_vec())),
            Value::Bool(false) => Ok(BoundValue::String(b"f".to_vec())),
            value => Ok(BoundValue::String(
                to_php_string(value, context.diagnostics())?
                    .as_bytes()
                    .split(|byte| *byte == 0)
                    .next()
                    .unwrap_or_default()
                    .to_vec(),
            )),
        })
        .collect()
}

fn text(arguments: &[Value], index: usize, call: &str, name: &str) -> Result<String, NativeError> {
    let text = args::string(arguments, index).ok_or_else(|| {
        NativeError::type_error(format!(
            "{call}(): Argument #{} (${name}) must be of type string, {} given",
            index + 1,
            args::value(arguments, index).deref().type_name()
        ))
    })?;
    let bytes = text.as_bytes();
    Ok(
        String::from_utf8_lossy(bytes.split(|byte| *byte == 0).next().unwrap_or_default())
            .into_owned(),
    )
}

pub fn execute<const OP: usize, const SEND: bool>(
    c: &mut dyn Context,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    let call = match (OP, SEND) {
        (SIMPLE, false) => "pg_query",
        (PARAMS, false) => "pg_query_params",
        (PREPARE, false) => "pg_prepare",
        (EXECUTE, false) => "pg_execute",
        (SIMPLE, true) => "pg_send_query",
        (PARAMS, true) => "pg_send_query_params",
        (PREPARE, true) => "pg_send_prepare",
        _ => "pg_send_execute",
    };
    execute_named::<OP, SEND>(c, a, call)
}

pub(crate) fn execute_named<'a, const OP: usize, const SEND: bool>(
    c: &'a mut dyn Context,
    a: Vec<Value>,
    call: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let explicit = args::object(&a, 0).is_some();
        let conn = if explicit {
            args::value(&a, 0)
        } else {
            &Value::Null
        };
        let command = Command::parse::<OP>(c, &a, usize::from(explicit), call)?;
        let state = connection::argument(c, conn, call)?;
        let lease = connection::lease(&state).await?;
        // A fork replays what the script changed about its session.
        let noted = match &command {
            Command::Simple(sql) if state.borrow().log.concerns(sql.as_bytes()) => {
                Some(sql.clone())
            }
            _ => None,
        };
        if SEND {
            let mut state = state.borrow_mut();
            if state.busy.is_some() || !state.results.is_empty() {
                c.warning(format!("{call}(): There are results on this connection. Call pg_get_result() until it returns FALSE"));
                return Ok(Value::Bool(false));
            }
            state.last_error.clear();
            state.busy = Some(
                connection::start(&lease, |driver| Box::pin(command.run(driver)))
                    .map_err(|error| NativeError::error(error.detail))?,
            );
            if let Some(sql) = noted {
                state.log.ran(sql.as_bytes());
            }
            return Ok(Value::Bool(true));
        }
        finish(&state).await?;
        {
            let mut state = state.borrow_mut();
            state.results.clear();
            state.last_error.clear();
        }
        let response = match command {
            Command::Simple(sql) => connection::run_simple(&lease, sql).await,
            command => connection::run(&lease, |driver| Box::pin(command.run(driver))).await,
        };
        match response.and_then(|mut rows| rows.pop().ok_or_else(|| DbError::general("No result")))
        {
            Ok(rows) if rows.error.is_none() => {
                if let Some(sql) = noted {
                    state.borrow_mut().log.ran(sql.as_bytes());
                }
                result::create(c, rows, Some(&state))
            }
            Ok(rows) => {
                let detail = rows.error.map(|error| error.detail).unwrap_or_default();
                fail(c, &state, call, detail)
            }
            Err(error) => fail(c, &state, call, error.detail),
        }
    })
}

fn fail(c: &mut dyn Context, state: &Handle, call: &str, detail: String) -> NativeResult {
    state.borrow_mut().last_error = detail.clone();
    c.warning(format!("{call}(): Query failed: {detail}"));
    Ok(Value::Bool(false))
}

async fn finish(state: &Handle) -> Result<(), NativeError> {
    let pending = state.borrow_mut().busy.take();
    if let Some(pending) = pending {
        let result = pending
            .await
            .map_err(|error| NativeError::error(error.to_string()))?;
        match result {
            Ok(results) => state.borrow_mut().results.extend(results),
            Err(e) => state.borrow_mut().results.push_back(Rows {
                error: Some(e),
                ..Rows::default()
            }),
        }
    }
    Ok(())
}

pub fn get(c: &mut dyn Context, a: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let state = connection::argument(c, args::value(&a, 0), "pg_get_result")?;
        state.borrow().ensure_open()?;
        finish(&state).await?;
        let result = state.borrow_mut().results.pop_front();
        if let Some(rows) = result {
            if let Some(error) = &rows.error {
                state.borrow_mut().last_error.clone_from(&error.detail);
            }
            result::create(c, rows, Some(&state))
        } else {
            Ok(Value::Bool(false))
        }
    })
}

pub fn busy(c: &mut dyn Context, a: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let state = connection::argument(c, args::value(&a, 0), "pg_connection_busy")?;
        state.borrow().ensure_open()?;
        poll_input(&state).await;
        Ok(Value::Bool(has_pending_query(&state)))
    })
}

pub fn consume(c: &mut dyn Context, a: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let state = connection::argument(c, args::value(&a, 0), "pg_consume_input")?;
        let lease = {
            let state = state.borrow();
            state.ensure_open()?;
            state.lease.clone()
        };
        let Some(lease) = lease else {
            return Ok(Value::Bool(true));
        };
        poll_input(&state).await;
        Ok(Value::Bool(
            lease
                .try_inspect(|driver| match driver {
                    phera_db_drivers::drivers::Driver::Postgres(pg) => !pg.is_broken(),
                    _ => false,
                })
                .unwrap_or(true),
        ))
    })
}

async fn poll_input(state: &Handle) {
    if has_pending_query(state) {
        // PHP may poll in a tight loop. The socket pump needs a turn on
        // this worker before its reply can become visible to the poller.
        tokio::task::yield_now().await;
    }
}

fn has_pending_query(state: &Handle) -> bool {
    state
        .borrow()
        .busy
        .as_ref()
        .is_some_and(|pending| !pending.is_finished())
}
