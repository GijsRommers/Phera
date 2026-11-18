//! `querySingle()` executes once; `query()` returns a reset cursor, as PHP does.

use crate::diagnostics::session;
use crate::statement::State;
use phera_extension_api::{Context, LocalFuture, NativeResult, Object, PhpArray, Value, args};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

async fn run(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    single: bool,
) -> NativeResult {
    let session = session(this.as_ref())?;
    let database = session.database()?;
    let sql = args::string(&arguments, 0)
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    if sql.is_empty() {
        return Ok(Value::Bool(false));
    }
    let method = if single {
        "SQLite3::querySingle"
    } else {
        "SQLite3::query"
    };
    let handle = match database.prepare(sql.clone()).await {
        Ok(handle) => handle,
        Err(error) => return session.fail(context, error, method, "Unable to prepare statement: "),
    };
    let execution = match handle.execute(Vec::new()).await {
        Ok(execution) => execution,
        Err(error) => return session.fail(context, error, method, "Unable to execute statement: "),
    };
    session.success();
    if single {
        return match (execution.first, args::bool(&arguments, 1).unwrap_or(false)) {
            (Some(row), true) => crate::result::row(row, &execution.description.columns, 1),
            (Some(row), false) => Ok(row
                .into_iter()
                .next()
                .map_or(Value::Null, crate::result::value)),
            (None, true) => Ok(Value::from(PhpArray::new())),
            (None, false) => Ok(Value::Null),
        };
    }
    handle
        .reset()
        .await
        .map_err(|e| phera_extension_api::NativeError::error(e.message))?;
    let state = Rc::new(RefCell::new(State {
        session: session.clone(),
        handle: Some(handle),
        sql,
        bindings: BTreeMap::new(),
        started: Cell::new(false),
        current: RefCell::new(None),
    }));
    session.statements.borrow_mut().push(Rc::downgrade(&state));
    crate::result::create(context, state, false)
}

pub fn query<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(run(context, this, arguments, false))
}
pub fn single<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(run(context, this, arguments, true))
}
