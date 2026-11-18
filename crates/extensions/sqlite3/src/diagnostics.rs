//! `$db->enableExceptions(true)` changes warnings into SQLite3Exception objects.

use phera_extension_api::{Context, NativeError, NativeResult, Object, Value};
use phera_sqlite::{Database, Error};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct Session {
    pub database: RefCell<Option<Database>>,
    pub exceptions: Cell<bool>,
    pub extended: Cell<bool>,
    pub error: RefCell<Error>,
    pub statements: RefCell<Vec<std::rc::Weak<RefCell<crate::statement::State>>>>,
}

impl Session {
    pub fn database(&self) -> Result<Database, NativeError> {
        self.database.borrow().clone().ok_or_else(|| {
            NativeError::error(
                "The SQLite3 object has not been correctly initialised or is already closed",
            )
        })
    }

    pub fn fail(
        &self,
        context: &mut dyn Context,
        error: Error,
        method: &str,
        prefix: &str,
    ) -> NativeResult {
        let message = format!("{prefix}{}", error.message);
        let code = if self.extended.get() {
            error.code
        } else {
            error.code & 255
        };
        *self.error.borrow_mut() = error;
        if self.exceptions.get() {
            Err(NativeError::throw_with_code(
                "SQLite3Exception",
                message,
                i64::from(code),
            ))
        } else {
            context.warning(format!("{method}(): {message}"));
            Ok(Value::Bool(false))
        }
    }

    pub fn stepped(&self, row: bool) {
        *self.error.borrow_mut() = Error {
            code: if row { 100 } else { 101 },
            message: if row {
                "another row available"
            } else {
                "no more rows available"
            }
            .into(),
        };
    }

    pub fn success(&self) {
        *self.error.borrow_mut() = Error {
            code: 0,
            message: "not an error".into(),
        };
    }
}

pub fn session(this: Option<&Rc<Object>>) -> Result<Rc<Session>, NativeError> {
    this.and_then(|object| {
        object
            .native
            .borrow()
            .as_ref()
            .and_then(|native| native.downcast_ref::<Rc<Session>>())
            .cloned()
    })
    .ok_or_else(|| {
        NativeError::error(
            "The SQLite3 object has not been correctly initialised or is already closed",
        )
    })
}
