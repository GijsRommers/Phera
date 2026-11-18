//! `bindParam(':id', $id, SQLITE3_INTEGER)` reads the reference each time execute runs.

use crate::diagnostics::{Session, session};
use phera_extension_api::value::{to_float, to_int, to_php_string};
use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args,
};
use phera_sqlite::{Cell as SqlCell, kind};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

pub struct State {
    pub session: Rc<Session>,
    pub handle: Option<phera_sqlite::Statement>,
    pub sql: String,
    pub bindings: BTreeMap<usize, (Value, i64)>,
    pub started: Cell<bool>,
    pub current: RefCell<Option<Vec<SqlCell>>>,
}

pub fn state(this: Option<&Rc<Object>>) -> Result<Rc<RefCell<State>>, NativeError> {
    let state = this
        .and_then(|object| {
            object
                .native
                .borrow()
                .as_ref()
                .and_then(|native| native.downcast_ref::<Rc<RefCell<State>>>())
                .cloned()
        })
        .ok_or_else(closed)?;
    if state.borrow().handle.is_none() {
        return Err(closed());
    }
    Ok(state)
}

pub(crate) fn closed() -> NativeError {
    NativeError::error(
        "The SQLite3Stmt object has not been correctly initialised or is already closed",
    )
}

pub fn prepare<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let session = session(this.as_ref())?;
        let sql = args::string(&arguments, 0)
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        session.database()?;
        if sql.is_empty() {
            return Ok(Value::Bool(false));
        }
        let handle = match session.database()?.prepare(sql.clone()).await {
            Ok(handle) => handle,
            Err(error) => {
                return session.fail(
                    context,
                    error,
                    "SQLite3::prepare",
                    "Unable to prepare statement: ",
                );
            }
        };
        session.success();
        let object = context.create_object("SQLite3Stmt", PhpArray::new())?;
        let Value::Object(instance) = &object else {
            return Err(NativeError::error("SQLite3Stmt could not be created"));
        };
        let state = Rc::new(RefCell::new(State {
            session: session.clone(),
            handle: Some(handle),
            sql,
            bindings: BTreeMap::new(),
            started: Cell::new(false),
            current: RefCell::new(None),
        }));
        session.statements.borrow_mut().push(Rc::downgrade(&state));
        *instance.native.borrow_mut() = Some(Box::new(state));
        Ok(object)
    })
}

pub fn bind(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let state = state(this)?;
    let mut state = state.borrow_mut();
    let handle = state.handle.as_ref().ok_or_else(closed)?;
    let index = match args::value(arguments, 0) {
        Value::Int(index) => usize::try_from(*index).ok(),
        Value::String(name) => {
            let name = name.to_string_lossy();
            let name = if name.starts_with(':') || name.starts_with('@') {
                name
            } else {
                format!(":{name}")
            };
            handle
                .description
                .parameters
                .iter()
                .position(|p| p.as_ref() == Some(&name))
                .map(|i| i + 1)
        }
        _ => None,
    };
    let Some(index) = index.filter(|i| *i > 0) else {
        return Ok(Value::Bool(false));
    };
    state.bindings.insert(
        index,
        (
            args::value(arguments, 1).clone(),
            args::int(arguments, 2).unwrap_or(3),
        ),
    );
    Ok(Value::Bool(true))
}

pub fn bindings(
    context: &mut dyn Context,
    state: &State,
) -> Result<Vec<(usize, SqlCell)>, NativeError> {
    let mut result = Vec::new();
    for (index, (value, type_code)) in &state.bindings {
        let source = value;
        let value = value.deref().clone();
        let value = if value.is_null() {
            SqlCell::Null
        } else {
            match *type_code {
                kind::INTEGER => SqlCell::Integer(to_int(&value, context.diagnostics())?),
                kind::FLOAT => SqlCell::Real(to_float(&value, context.diagnostics())?),
                kind::TEXT => SqlCell::Text(
                    to_php_string(&value, context.diagnostics())?
                        .as_bytes()
                        .to_vec(),
                ),
                kind::BLOB => SqlCell::Blob(
                    to_php_string(&value, context.diagnostics())?
                        .as_bytes()
                        .to_vec(),
                ),
                kind::NULL => SqlCell::Null,
                _ => return Err(NativeError::value_error("Unknown parameter type")),
            }
        };
        if let Value::Reference(reference) = source {
            let converted = match &value {
                SqlCell::Integer(value) => Some(Value::Int(*value)),
                SqlCell::Real(value) => Some(Value::Float(*value)),
                _ => None,
            };
            // Release the read borrow before applying PHP's numeric conversion in place.
            if let Some(converted) = converted {
                *reference.borrow_mut() = converted;
            }
        }
        result.push((*index, value));
    }
    Ok(result)
}

pub fn execute<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = state(this.as_ref()).map_err(|_| {
            NativeError::error(
                "The SQLite3 object has not been correctly initialised or is already closed",
            )
        })?;
        let (handle, session, values) = {
            let state = state.borrow();
            (
                state.handle.clone().ok_or_else(closed)?,
                state.session.clone(),
                bindings(context, &state)?,
            )
        };
        match handle.execute(values).await {
            Ok(_) => {
                handle
                    .reset()
                    .await
                    .map_err(|e| NativeError::error(e.message))?;
                state.borrow().session.success();
                state.borrow().started.set(false);
                state.borrow().current.replace(None);
                session.success();
                crate::result::create(context, state, true)
            }
            Err(error) => session.fail(
                context,
                error,
                "SQLite3Stmt::execute",
                "Unable to execute statement: ",
            ),
        }
    })
}

pub fn reset<'a>(
    _: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = state(this.as_ref()).map_err(|_| {
            NativeError::error(
                "The SQLite3 object has not been correctly initialised or is already closed",
            )
        })?;
        let handle = state.borrow().handle.clone().ok_or_else(closed)?;
        handle
            .reset()
            .await
            .map_err(|e| NativeError::error(e.message))?;
        state.borrow().session.success();
        state.borrow().started.set(false);
        state.borrow().current.replace(None);
        Ok(Value::Bool(true))
    })
}

pub fn clear<'a>(
    _: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = state(this.as_ref()).map_err(|_| {
            NativeError::error(
                "The SQLite3 object has not been correctly initialised or is already closed",
            )
        })?;
        let handle = state.borrow().handle.clone().ok_or_else(closed)?;
        handle
            .clear()
            .await
            .map_err(|e| NativeError::error(e.message))?;
        state.borrow_mut().bindings.clear();
        Ok(Value::Bool(true))
    })
}

pub fn close<'a>(
    _: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = state(this.as_ref()).map_err(|_| {
            NativeError::error(
                "The SQLite3 object has not been correctly initialised or is already closed",
            )
        })?;
        let handle = state.borrow_mut().handle.take().ok_or_else(closed)?;
        handle
            .reset()
            .await
            .map_err(|e| NativeError::error(e.message))?;
        Ok(Value::Bool(true))
    })
}
