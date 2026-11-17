//! `$pdo->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT)` changes later failures.

use std::rc::Rc;

use phera_extension_api::value::{to_bool, to_int};
use phera_extension_api::{Context, NativeError, NativeResult, Object, Value, args};

use crate::connection::session;
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
            3 => {
                self.errors = match to_int(value, context.diagnostics())? {
                    0 => ErrorMode::Silent,
                    1 => ErrorMode::Warning,
                    2 => ErrorMode::Exception,
                    _ => {
                        return Err(NativeError::value_error(
                            "Error mode must be one of the PDO::ERRMODE_* constants",
                        ));
                    }
                }
            }
            8 => {
                self.case = match to_int(value, context.diagnostics())? {
                    0 => ColumnCase::Natural,
                    1 => ColumnCase::Upper,
                    2 => ColumnCase::Lower,
                    _ => return Err(NativeError::value_error("Invalid case folding mode")),
                }
            }
            17 => self.stringify = to_bool(value),
            19 => self.fetch = FetchMode::parse(to_int(value, context.diagnostics())?, self.fetch)?,
            20 => self.emulate = to_bool(value),
            12 => self.persistent = to_bool(value),
            // MySQL's driver constants for SSL options must not be silently ignored.
            1007..=1011 | 1014 => return Err(unsupported("TLS")),
            _ => return Err(unsupported("This PDO attribute")),
        }
        Ok(())
    }
}

pub fn set(context: &mut dyn Context, this: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    let connection = session(this)?;
    connection.attributes.borrow_mut().set(
        context,
        phera_extension_api::args::int(args, 0).unwrap_or(0),
        phera_extension_api::args::value(args, 1),
    )?;
    Ok(Value::Bool(true))
}

pub fn get(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let connection = session(this)?;
    let attributes = connection.attributes.borrow();
    let value = match args::int(arguments, 0).unwrap_or(0) {
        3 => Value::Int(match attributes.errors {
            ErrorMode::Silent => 0,
            ErrorMode::Warning => 1,
            ErrorMode::Exception => 2,
        }),
        4 => Value::from(connection.version.clone()),
        8 => Value::Int(match attributes.case {
            ColumnCase::Natural => 0,
            ColumnCase::Upper => 1,
            ColumnCase::Lower => 2,
        }),
        12 => Value::Bool(attributes.persistent),
        16 => Value::from(connection.name.name()),
        17 => Value::Bool(attributes.stringify),
        19 => Value::Int(attributes.fetch.number()),
        20 => Value::Bool(attributes.emulate),
        _ => return Err(unsupported("This PDO attribute")),
    };
    Ok(value)
}
