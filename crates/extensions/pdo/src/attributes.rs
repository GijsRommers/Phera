//! `$pdo->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT)` changes later failures.

use std::rc::Rc;

use phera_extension_api::value::{to_bool, to_int};
use phera_extension_api::{Context, NativeError, NativeResult, Object, Value, args};

use crate::connection::session;
use crate::constants::{
    ATTR_CASE, ATTR_DEFAULT_FETCH_MODE, ATTR_DRIVER_NAME, ATTR_EMULATE_PREPARES, ATTR_ERRMODE,
    ATTR_PERSISTENT, ATTR_SERVER_VERSION, ATTR_STRINGIFY_FETCHES, CASE_LOWER, CASE_NATURAL,
    CASE_UPPER, ERRMODE_EXCEPTION, ERRMODE_SILENT, ERRMODE_WARNING,
};
use crate::dsn::DriverName;
use crate::errors::{ErrorMode, unsupported};
use crate::fetch::FetchMode;

#[derive(Clone, Copy)]
pub enum ColumnCase {
    Natural,
    Upper,
    Lower,
}

#[derive(Clone)]
pub struct Attributes {
    pub errors: ErrorMode,
    pub fetch: FetchMode,
    pub emulate: bool,
    pub stringify: bool,
    pub case: ColumnCase,
    pub persistent: bool,
}

impl Attributes {
    pub fn new(driver: DriverName) -> Self {
        Self {
            errors: ErrorMode::Exception,
            fetch: FetchMode::Both,
            emulate: driver == DriverName::Mysql,
            stringify: false,
            case: ColumnCase::Natural,
            persistent: false,
        }
    }

    pub fn set(
        &mut self,
        context: &mut dyn Context,
        attribute: i64,
        value: &Value,
    ) -> Result<(), NativeError> {
        match attribute {
            ATTR_ERRMODE => {
                self.errors = match to_int(value, context.diagnostics())? {
                    ERRMODE_SILENT => ErrorMode::Silent,
                    ERRMODE_WARNING => ErrorMode::Warning,
                    ERRMODE_EXCEPTION => ErrorMode::Exception,
                    _ => {
                        return Err(NativeError::value_error(
                            "Error mode must be one of the PDO::ERRMODE_* constants",
                        ));
                    }
                }
            }
            ATTR_CASE => {
                self.case = match to_int(value, context.diagnostics())? {
                    CASE_NATURAL => ColumnCase::Natural,
                    CASE_UPPER => ColumnCase::Upper,
                    CASE_LOWER => ColumnCase::Lower,
                    _ => return Err(NativeError::value_error("Invalid case folding mode")),
                }
            }
            ATTR_STRINGIFY_FETCHES => self.stringify = to_bool(value),
            ATTR_DEFAULT_FETCH_MODE => {
                self.fetch = FetchMode::parse(to_int(value, context.diagnostics())?, self.fetch)?
            }
            ATTR_EMULATE_PREPARES => self.emulate = to_bool(value),
            ATTR_PERSISTENT => self.persistent = to_bool(value),
            // MySQL's driver constants for SSL options must not be silently ignored.
            1007..=1011 | 1014 => return Err(unsupported("TLS")),
            _ => return Err(unsupported("This PDO attribute")),
        }
        Ok(())
    }
}

pub fn set(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let connection = session(this)?;
    connection.attributes.borrow_mut().set(
        context,
        args::int(arguments, 0).unwrap_or_default(),
        args::value(arguments, 1),
    )?;
    Ok(Value::Bool(true))
}

pub fn get(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let connection = session(this)?;
    let attributes = connection.attributes.borrow();
    let value = match args::int(arguments, 0).unwrap_or(0) {
        ATTR_ERRMODE => Value::Int(match attributes.errors {
            ErrorMode::Silent => ERRMODE_SILENT,
            ErrorMode::Warning => ERRMODE_WARNING,
            ErrorMode::Exception => ERRMODE_EXCEPTION,
        }),
        ATTR_SERVER_VERSION => Value::from(connection.version.clone()),
        ATTR_CASE => Value::Int(match attributes.case {
            ColumnCase::Natural => CASE_NATURAL,
            ColumnCase::Upper => CASE_UPPER,
            ColumnCase::Lower => CASE_LOWER,
        }),
        ATTR_PERSISTENT => Value::Bool(attributes.persistent),
        ATTR_DRIVER_NAME => Value::from(connection.name.name()),
        ATTR_STRINGIFY_FETCHES => Value::Bool(attributes.stringify),
        ATTR_DEFAULT_FETCH_MODE => Value::Int(attributes.fetch.number()),
        ATTR_EMULATE_PREPARES => Value::Bool(attributes.emulate),
        _ => return Err(unsupported("This PDO attribute")),
    };
    Ok(value)
}
