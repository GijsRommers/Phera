//! Opening, executing and closing SQLite databases always awaits the database thread.

use crate::diagnostics::{Session, session};
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value, args};
use phera_sqlite::{Database, Error};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub fn open<'a>(
    _: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let object = this.ok_or_else(|| NativeError::error("SQLite3 object is missing"))?;
        let previous = session(Some(&object)).ok();
        if previous
            .as_ref()
            .is_some_and(|s| s.database.borrow().is_some())
        {
            return Err(NativeError::error("Already initialised DB Object"));
        }
        let filename = args::string(&arguments, 0)
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        if filename.contains('\0') {
            return Err(NativeError::value_error(
                "SQLite3::open(): Argument #1 ($filename) must not contain any null bytes",
            ));
        }
        let database = Database::open(filename, args::int(&arguments, 1).unwrap_or(6) as i32, 0)
            .await
            .map_err(|error| {
                NativeError::throw_with_code(
                    "SQLite3Exception",
                    format!("Unable to open database: {}", error.message),
                    i64::from(error.code & 255),
                )
            })?;
        let session = previous.unwrap_or_else(|| {
            Rc::new(Session {
                database: RefCell::new(None),
                exceptions: Cell::new(false),
                extended: Cell::new(false),
                error: RefCell::new(Error {
                    code: 0,
                    message: "not an error".into(),
                }),
                statements: RefCell::new(Vec::new()),
            })
        });
        *session.database.borrow_mut() = Some(database);
        *object.native.borrow_mut() = Some(Box::new(session));
        Ok(Value::Null)
    })
}

pub fn close<'a>(
    _: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let session = session(this.as_ref())?;
        let statements: Vec<_> = session
            .statements
            .borrow()
            .iter()
            .filter_map(|s| s.upgrade())
            .collect();
        for statement in statements {
            let handle = statement.borrow_mut().handle.take();
            if let Some(handle) = handle {
                let _ = handle.reset().await;
            }
        }
        let database = session.database.borrow_mut().take();
        if let Some(database) = database {
            database
                .close()
                .await
                .map_err(|e| NativeError::error(e.message))?;
        }
        Ok(Value::Bool(true))
    })
}

pub fn exec<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let session = session(this.as_ref())?;
        let sql = args::string(&arguments, 0)
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        match session.database()?.exec(sql).await {
            Ok(_) => {
                session.success();
                Ok(Value::Bool(true))
            }
            Err(error) => session.fail(context, error, "SQLite3::exec", ""),
        }
    })
}

pub fn timeout<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let session = session(this.as_ref())?;
        let milliseconds = args::int(&arguments, 0).unwrap_or(0).max(0) as u64;
        match session.database()?.timeout(milliseconds).await {
            Ok(()) => Ok(Value::Bool(true)),
            Err(error) => session.fail(context, error, "SQLite3::busyTimeout", ""),
        }
    })
}

pub fn last_id<'a>(
    _: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let id = session(this.as_ref())?
            .database()?
            .last_id()
            .await
            .map_err(|e| NativeError::error(e.message))?;
        Ok(Value::Int(id))
    })
}

pub fn changes<'a>(
    _: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let changes = session(this.as_ref())?
            .database()?
            .changes()
            .await
            .map_err(|e| NativeError::error(e.message))?;
        Ok(Value::Int(changes as i64))
    })
}
