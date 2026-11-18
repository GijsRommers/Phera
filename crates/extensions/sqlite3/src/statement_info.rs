//! `getSQL(true)` binds current values without executing the statement.
use crate::statement::{bindings, closed, state};
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value, args};
use std::rc::Rc;

pub fn count(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let state = state(this)?;
    Ok(Value::Int(
        state
            .borrow()
            .handle
            .as_ref()
            .ok_or_else(closed)?
            .description
            .parameters
            .len() as i64,
    ))
}

pub fn readonly(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let state = state(this)?;
    Ok(Value::Bool(
        state
            .borrow()
            .handle
            .as_ref()
            .ok_or_else(closed)?
            .description
            .readonly,
    ))
}

pub fn busy(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let state = state(this)?;
    Ok(Value::Bool(state.borrow().current.borrow().is_some()))
}

pub fn explain(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let state = state(this)?;
    Ok(Value::Int(i64::from(
        state
            .borrow()
            .handle
            .as_ref()
            .ok_or_else(closed)?
            .description
            .explain,
    )))
}

pub fn sql<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = state(this.as_ref()).map_err(|_| {
            NativeError::error(
                "The SQLite3 object has not been correctly initialised or is already closed",
            )
        })?;
        let (handle, bound) = {
            let s = state.borrow();
            (s.handle.clone().ok_or_else(closed)?, bindings(context, &s)?)
        };
        handle
            .bind(bound)
            .await
            .map_err(|e| NativeError::error(e.message))?;
        if !args::bool(&arguments, 0).unwrap_or(false) {
            return Ok(Value::from(state.borrow().sql.clone()));
        }
        let handle = state.borrow().handle.clone().ok_or_else(closed)?;
        Ok(handle
            .expanded_sql()
            .await
            .map_err(|e| NativeError::error(e.message))?
            .map_or(Value::Bool(false), Value::from))
    })
}
