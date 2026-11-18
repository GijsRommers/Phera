//! `fetchArray(SQLITE3_BOTH)` emits the numeric key before the column name.

use crate::statement::State;
use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args,
};
use phera_sqlite::{Cell, Column};
use std::cell::RefCell;
use std::rc::Rc;

pub struct ResultState {
    pub statement: Rc<RefCell<State>>,
    pub prepared: bool,
}

pub fn create(
    context: &mut dyn Context,
    statement: Rc<RefCell<State>>,
    prepared: bool,
) -> NativeResult {
    let value = context.create_object("SQLite3Result", PhpArray::new())?;
    let Value::Object(object) = &value else {
        return Err(NativeError::error("SQLite3Result could not be created"));
    };
    *object.native.borrow_mut() = Some(Box::new(Rc::new(ResultState {
        statement,
        prepared,
    })));
    Ok(value)
}

fn state(this: Option<&Rc<Object>>) -> Result<Rc<ResultState>, NativeError> {
    let state = this
        .and_then(|object| {
            object
                .native
                .borrow()
                .as_ref()
                .and_then(|native| native.downcast_ref::<Rc<ResultState>>())
                .cloned()
        })
        .ok_or_else(closed)?;
    if state.statement.borrow().handle.is_none() {
        return Err(closed());
    }
    Ok(state)
}

fn closed() -> NativeError {
    NativeError::error(
        "The SQLite3Result object has not been correctly initialised or is already closed",
    )
}

pub fn row(values: Vec<Cell>, columns: &[Column], mode: i64) -> NativeResult {
    let mut result = PhpArray::new();
    for (index, (cell, column)) in values.into_iter().zip(columns).enumerate() {
        let value = value(cell);
        if mode & 2 != 0 {
            result.insert(ArrayKey::Int(index as i64), value.clone());
        }
        if mode & 1 != 0 {
            result.insert(
                ArrayKey::from_value(&Value::from(column.name.clone()))?,
                value,
            );
        }
    }
    Ok(Value::from(result))
}

pub fn value(cell: Cell) -> Value {
    match cell {
        Cell::Null => Value::Null,
        Cell::Integer(value) => Value::Int(value),
        Cell::Real(value) => Value::Float(value),
        Cell::Text(value) => {
            Value::from(value.split(|b| *b == 0).next().unwrap_or_default().to_vec())
        }
        Cell::Blob(value) => Value::from(value),
    }
}

pub fn fetch<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = state(this.as_ref())?;
        let (handle, started, session) = {
            let s = state.statement.borrow();
            (
                s.handle.clone().ok_or_else(closed)?,
                s.started.get(),
                s.session.clone(),
            )
        };
        let result = if started {
            handle.fetch().await
        } else {
            handle.execute(Vec::new()).await.map(|e| e.first)
        };
        match result {
            Ok(values) => {
                session.stepped(values.is_some());
                let s = state.statement.borrow();
                // SQLite automatically restarts a completed statement on its next step.
                s.started.set(values.is_some());
                s.current.replace(values.clone());
                match values {
                    Some(values) => row(
                        values,
                        &handle.description.columns,
                        args::int(&arguments, 0).unwrap_or(3),
                    ),
                    None => Ok(Value::Bool(false)),
                }
            }
            Err(error) => session.fail(
                context,
                error,
                "SQLite3Result::fetchArray",
                "Unable to execute statement: ",
            ),
        }
    })
}

pub fn all<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let mut rows = PhpArray::new();
        loop {
            let row = fetch(context, this.clone(), arguments.clone()).await?;
            if matches!(row, Value::Bool(false)) {
                break;
            }
            rows.push(row)?;
        }
        Ok(Value::from(rows))
    })
}

pub fn count(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let state = state(this)?;
    let s = state.statement.borrow();
    Ok(Value::Int(
        s.handle
            .as_ref()
            .ok_or_else(closed)?
            .description
            .columns
            .len() as i64,
    ))
}

pub fn name(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let state = state(this)?;
    let s = state.statement.borrow();
    Ok(usize::try_from(args::int(arguments, 0).unwrap_or(0))
        .ok()
        .and_then(|i| s.handle.as_ref()?.description.columns.get(i))
        .map_or(Value::Bool(false), |c| Value::from(c.name.clone())))
}

pub fn kind(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let state = state(this)?;
    let s = state.statement.borrow();
    let current = s.current.borrow();
    let Some(values) = current.as_ref() else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Int(
        usize::try_from(args::int(arguments, 0).unwrap_or(0))
            .ok()
            .and_then(|i| values.get(i))
            .map_or(phera_sqlite::kind::NULL, Cell::kind),
    ))
}

pub fn reset<'a>(
    _: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = state(this.as_ref())?;
        let handle = state.statement.borrow().handle.clone().ok_or_else(closed)?;
        handle
            .reset()
            .await
            .map_err(|e| NativeError::error(e.message))?;
        state.statement.borrow().session.success();
        state.statement.borrow().started.set(false);
        state.statement.borrow().current.replace(None);
        Ok(Value::Bool(true))
    })
}

pub fn finalize<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = state(this.as_ref())?;
        reset(context, this, args).await?;
        if !state.prepared {
            state.statement.borrow_mut().handle.take();
        }
        Ok(Value::Bool(true))
    })
}
