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
    driver: DriverName,
    pub errors: ErrorMode,
    pub fetch: FetchMode,
    pub emulate: bool,
    pub stringify: bool,
    pub case: ColumnCase,
    pub persistent: bool,
    pub timeout: i64,
    pub sqlite_flags: i64,
    pub extended: bool,
    pub transaction_mode: i64,
}

impl Attributes {
    pub fn new(driver: DriverName) -> Self {
        Self {
            driver,
            errors: ErrorMode::Exception,
            fetch: FetchMode::Both,
            emulate: driver == DriverName::Mysql,
            stringify: false,
            case: ColumnCase::Natural,
            persistent: false,
            timeout: 60,
            sqlite_flags: 6,
            extended: false,
            transaction_mode: 0,
        }
    }

    pub fn set(
        &mut self,
        context: &mut dyn Context,
        attribute: i64,
        value: &Value,
    ) -> Result<(), NativeError> {
        match attribute {
            2 if self.driver == DriverName::Sqlite => {
                self.timeout = to_int(value, context.diagnostics())?
            }
            1000 if self.driver == DriverName::Sqlite => {
                self.sqlite_flags = to_int(value, context.diagnostics())?
            }
            1002 if self.driver == DriverName::Sqlite => self.extended = to_bool(value),
            1005 if self.driver == DriverName::Sqlite => {
                self.transaction_mode = to_int(value, context.diagnostics())?
            }
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

pub fn set<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> phera_extension_api::LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let connection = session(this.as_ref())?;
        let attribute = args::int(&arguments, 0).unwrap_or(0);
        if connection.name == DriverName::Sqlite {
            if !matches!(attribute, 2 | 3 | 8 | 12 | 17 | 19 | 1002 | 1005) {
                return Ok(Value::Bool(false));
            }
            if attribute == 1005 {
                let mode = to_int(args::value(&arguments, 1), context.diagnostics())?;
                if !(0..=2).contains(&mode) {
                    return Ok(Value::Bool(false));
                }
            }
            if attribute == 2 {
                let seconds = to_int(args::value(&arguments, 1), context.diagnostics())?;
                let result = connection
                    .driver
                    .lock()
                    .await
                    .sqlite_timeout(seconds.max(0).saturating_mul(1000) as u64)
                    .await;
                if let Err(error) = result {
                    return connection.fail(context, error, "PDO::setAttribute");
                }
            }
        }
        connection
            .attributes
            .borrow_mut()
            .set(context, attribute, args::value(&arguments, 1))?;
        Ok(Value::Bool(true))
    })
}

pub fn get(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let connection = session(this)?;
    let attribute = args::int(arguments, 0).unwrap_or(0);
    if connection.name == DriverName::Sqlite
        && !matches!(attribute, 3 | 4 | 5 | 8 | 12 | 16 | 17 | 19 | 1005)
    {
        return connection.fail(
            context,
            crate::errors::DbError {
                state: "IM001".into(),
                code: None,
                detail: "driver does not support that attribute".into(),
            },
            "PDO::getAttribute",
        );
    }
    let attributes = connection.attributes.borrow();
    let value = match args::int(arguments, 0).unwrap_or(0) {
        3 => Value::Int(match attributes.errors {
            ErrorMode::Silent => 0,
            ErrorMode::Warning => 1,
            ErrorMode::Exception => 2,
        }),
        4 | 5 => Value::from(connection.version.clone()),
        1005 => Value::Int(attributes.transaction_mode),
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
