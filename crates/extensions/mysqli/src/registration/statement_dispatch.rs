//! `mysqli_stmt_execute($stmt)` and `$stmt->execute()` share the same prepared handle.

use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value, args};
use std::rc::Rc;

use super::dispatch::target;
use crate::{connection, result, statements};

pub const PREPARE: usize = 0;
pub const EXECUTE: usize = 1;
pub const QUERY: usize = 2;

pub fn init_method(c: &mut dyn Context, o: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    statements::init(c, connection::handle(o)?)
}

pub fn init_proc(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    init_method(c, args::object(a, 0), &[])
}

pub fn construct(
    c: &mut dyn Context,
    o: Option<Rc<Object>>,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let o = o.as_ref().ok_or_else(missing)?;
        let connection = connection::handle(args::object(&a, 0))?;
        connection.borrow().lease()?;
        let state = statements::attach(o, connection);
        if let Some(sql) = args::string(&a, 1) {
            statements::prepare(
                c,
                state,
                sql.as_bytes().to_vec(),
                "mysqli_stmt::__construct",
            )
            .await?;
        }
        Ok(Value::Null)
    })
}

pub fn connection_call<const OP: usize, const METHOD: bool>(
    c: &mut dyn Context,
    o: Option<Rc<Object>>,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let (o, a) = target(o.as_ref(), &a);
        let connection = connection::handle(o)?;
        let value = statements::init(c, connection)?;
        let Some(o) = args::object(std::slice::from_ref(&value), 0) else {
            return Err(missing());
        };
        let state = statements::handle(o)?;
        let call = match (OP, METHOD) {
            (PREPARE, true) => "mysqli::prepare",
            (PREPARE, false) => "mysqli_prepare",
            (_, true) => "mysqli::execute_query",
            (_, false) => "mysqli_execute_query",
        };
        let prepared =
            statements::prepare(c, state.clone(), args::bytes(a, 0).to_vec(), call).await?;
        if !matches!(prepared, Value::Bool(true)) {
            return Ok(prepared);
        }
        if OP == PREPARE {
            return Ok(value);
        }
        let result =
            statements::execute(c, state.clone(), args::array(a, 1).map(Rc::as_ref), call).await?;
        if !matches!(result, Value::Bool(true)) {
            return Ok(result);
        }
        let rows = state.borrow_mut().rows.take();
        if let Some(rows) = rows.filter(|rows| !rows.columns.is_empty()) {
            return result::create_rows(c, rows, false);
        }
        Ok(Value::Bool(true))
    })
}

pub fn connection_proc<const OP: usize>(
    c: &mut dyn Context,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    connection_call::<OP, false>(c, None, a)
}

pub fn execute<const OP: usize, const METHOD: bool>(
    c: &mut dyn Context,
    o: Option<Rc<Object>>,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let (o, a) = target(o.as_ref(), &a);
        let state = statements::handle(o.ok_or_else(missing)?)?;
        match OP {
            PREPARE => {
                statements::prepare(
                    c,
                    state,
                    args::bytes(a, 0).to_vec(),
                    if METHOD {
                        "mysqli_stmt::prepare"
                    } else {
                        "mysqli_stmt_prepare"
                    },
                )
                .await
            }
            _ => {
                statements::execute(
                    c,
                    state,
                    args::array(a, 0).map(Rc::as_ref),
                    if METHOD {
                        "mysqli_stmt::execute"
                    } else {
                        "mysqli_stmt_execute"
                    },
                )
                .await
            }
        }
    })
}

pub fn execute_proc<const OP: usize>(
    c: &mut dyn Context,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    execute::<OP, false>(c, None, a)
}

pub fn method<const OP: usize>(
    c: &mut dyn Context,
    o: Option<&Rc<Object>>,
    a: &[Value],
) -> NativeResult {
    local::<OP>(c, o.ok_or_else(missing)?, a, true)
}

pub fn procedure<const OP: usize>(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    local::<OP>(
        c,
        args::object(a, 0).ok_or_else(missing)?,
        a.get(1..).unwrap_or_default(),
        false,
    )
}

fn local<const OP: usize>(
    c: &mut dyn Context,
    o: &Rc<Object>,
    a: &[Value],
    method: bool,
) -> NativeResult {
    let names = [
        "bind_param",
        "bind_result",
        "fetch",
        "store_result",
        "get_result",
        "result_metadata",
        "send_long_data",
        "reset",
        "close",
        "free_result",
        "data_seek",
        "more_results",
        "next_result",
    ];
    let suffix = names.get(OP).copied().unwrap_or_default();
    let name = format!(
        "{}{suffix}",
        if method {
            "mysqli_stmt::"
        } else {
            "mysqli_stmt_"
        }
    );
    statements::methods::execute::<OP>(c, o, a, &name)
}

pub fn property<const OP: usize>(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    statements::properties::value(args::object(a, 0).ok_or_else(missing)?, OP)
}

pub fn num_rows(_: &mut dyn Context, object: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    statements::properties::value(object.ok_or_else(missing)?, 2)
}

fn missing() -> NativeError {
    NativeError::error("mysqli_stmt object is missing")
}
