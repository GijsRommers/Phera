//! `$pdo->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT)` changes later failures.

use std::rc::Rc;

use phera_extension_api::value::{NumVal, parse_numeric, to_bool, to_int};
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value, args};

use crate::connection::session;
use crate::constants::{
    ATTR_CASE, ATTR_CLIENT_VERSION, ATTR_DEFAULT_FETCH_MODE, ATTR_DRIVER_NAME,
    ATTR_EMULATE_PREPARES, ATTR_ERRMODE, ATTR_ORACLE_NULLS, ATTR_PERSISTENT, ATTR_SERVER_VERSION,
    ATTR_STRINGIFY_FETCHES, ATTR_TIMEOUT, CASE_LOWER, CASE_NATURAL, CASE_UPPER, ERRMODE_EXCEPTION,
    ERRMODE_SILENT, ERRMODE_WARNING, SQLITE_ATTR_EXTENDED_RESULT_CODES, SQLITE_ATTR_OPEN_FLAGS,
    SQLITE_ATTR_TRANSACTION_MODE, SQLITE_OPEN_CREATE, SQLITE_OPEN_READWRITE,
};
use crate::dsn::DriverName;
use crate::errors::{DbError, ErrorMode, unsupported};
use crate::fetch::FetchMode;

/// pdo_mysql's TLS options, `Pdo\Mysql::ATTR_SSL_KEY` (1006) to
/// `ATTR_SSL_CIPHER` (1010), and `ATTR_SSL_VERIFY_SERVER_CERT`. Phera does
/// not register `Pdo\Mysql`'s constants yet, so only the numbers reach it.
const MYSQL_ATTR_SSL_KEY: i64 = 1006;
const MYSQL_ATTR_SSL_CIPHER: i64 = 1010;
const MYSQL_ATTR_SSL_VERIFY_SERVER_CERT: i64 = 1013;

#[derive(Clone, Copy)]
pub enum ColumnCase {
    Natural,
    Upper,
    Lower,
}

/// The attributes pdo_sqlite accepts in `setAttribute()`; the constructor
/// also takes `ATTR_OPEN_FLAGS`. Any other is refused with false.
const SQLITE_SETTABLE: [i64; 9] = [
    ATTR_TIMEOUT,
    ATTR_ERRMODE,
    ATTR_CASE,
    ATTR_ORACLE_NULLS,
    ATTR_PERSISTENT,
    ATTR_STRINGIFY_FETCHES,
    ATTR_DEFAULT_FETCH_MODE,
    SQLITE_ATTR_EXTENDED_RESULT_CODES,
    SQLITE_ATTR_TRANSACTION_MODE,
];

/// The attributes pdo_sqlite answers in `getAttribute()`.
const SQLITE_READABLE: [i64; 10] = [
    ATTR_ERRMODE,
    ATTR_SERVER_VERSION,
    ATTR_CLIENT_VERSION,
    ATTR_CASE,
    ATTR_ORACLE_NULLS,
    ATTR_PERSISTENT,
    ATTR_DRIVER_NAME,
    ATTR_STRINGIFY_FETCHES,
    ATTR_DEFAULT_FETCH_MODE,
    SQLITE_ATTR_TRANSACTION_MODE,
];

/// The seconds SQLite waits for a lock before it reports "database is locked".
const SQLITE_DEFAULT_TIMEOUT: i64 = 60;

#[derive(Clone)]
pub struct Attributes {
    driver: DriverName,
    pub errors: ErrorMode,
    pub fetch: FetchMode,
    pub emulate: bool,
    pub stringify: bool,
    pub case: ColumnCase,
    pub oracle_nulls: i64,
    pub persistent: bool,
    /// SQLite only: the lock timeout in seconds, the open flags, whether
    /// errors report SQLite's extended result codes, and the BEGIN mode.
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
            oracle_nulls: 0,
            persistent: false,
            timeout: SQLITE_DEFAULT_TIMEOUT,
            sqlite_flags: SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE,
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
        let sqlite = self.driver == DriverName::Sqlite;
        match attribute {
            ATTR_TIMEOUT if sqlite => self.timeout = to_int(value, context.diagnostics())?,
            SQLITE_ATTR_OPEN_FLAGS if sqlite => {
                self.sqlite_flags = to_int(value, context.diagnostics())?
            }
            SQLITE_ATTR_EXTENDED_RESULT_CODES if sqlite => self.extended = to_bool(value),
            SQLITE_ATTR_TRANSACTION_MODE if sqlite => {
                self.transaction_mode = to_int(value, context.diagnostics())?
            }
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
            ATTR_ORACLE_NULLS => self.oracle_nulls = integer(value)?,
            ATTR_DEFAULT_FETCH_MODE => {
                self.fetch = FetchMode::parse(to_int(value, context.diagnostics())?, self.fetch)?
            }
            ATTR_EMULATE_PREPARES => self.emulate = to_bool(value),
            ATTR_PERSISTENT => self.persistent = to_bool(value),
            // A TLS option must not be ignored silently.
            MYSQL_ATTR_SSL_KEY..=MYSQL_ATTR_SSL_CIPHER | MYSQL_ATTR_SSL_VERIFY_SERVER_CERT => {
                return Err(unsupported("TLS"));
            }
            _ => return Err(unsupported("This PDO attribute")),
        }
        Ok(())
    }
}

pub(crate) fn integer(value: &Value) -> Result<i64, NativeError> {
    match &*value.deref() {
        Value::Int(value) => return Ok(*value),
        Value::Bool(value) => return Ok(i64::from(*value)),
        Value::String(value) => {
            let parsed = parse_numeric(value.as_bytes());
            if parsed.exact
                && let Some(NumVal::Int(value)) = parsed.value
            {
                return Ok(value);
            }
        }
        _ => {}
    }
    Err(NativeError::type_error(format!(
        "Attribute value must be of type int for selected attribute, {} given",
        value.type_name()
    )))
}

/// Whether the constructor's options may set `attribute`: for SQLite only
/// the ones pdo_sqlite knows.
pub fn constructor_accepts(driver: DriverName, attribute: i64) -> bool {
    driver != DriverName::Sqlite
        || attribute == SQLITE_ATTR_OPEN_FLAGS
        || SQLITE_SETTABLE.contains(&attribute)
}

/// `setAttribute()`. Asynchronous because SQLite's lock timeout is set on
/// the database's own thread.
pub fn set<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let connection = session(this.as_ref())?;
        let attribute = args::int(&arguments, 0).unwrap_or_default();
        let value = args::value(&arguments, 1);
        if connection.name == DriverName::Sqlite {
            if !SQLITE_SETTABLE.contains(&attribute) {
                return Ok(Value::Bool(false));
            }
            if attribute == SQLITE_ATTR_TRANSACTION_MODE
                && !(0..=SQLITE_TRANSACTION_MODES).contains(&to_int(value, context.diagnostics())?)
            {
                return Ok(Value::Bool(false));
            }
            if attribute == ATTR_TIMEOUT {
                let seconds = to_int(value, context.diagnostics())?;
                let milliseconds = seconds.max(0).saturating_mul(1000) as u64;
                let result = connection
                    .driver
                    .lock()
                    .await
                    .sqlite_timeout(milliseconds)
                    .await;
                if let Err(error) = result {
                    return connection.fail(context, error, "PDO::setAttribute");
                }
            }
        }
        connection
            .attributes
            .borrow_mut()
            .set(context, attribute, value)?;
        Ok(Value::Bool(true))
    })
}

/// The highest `Pdo\Sqlite::TRANSACTION_MODE_*`.
const SQLITE_TRANSACTION_MODES: i64 = 2;

pub fn get(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let connection = session(this)?;
    let attribute = args::int(arguments, 0).unwrap_or_default();
    if connection.name == DriverName::Sqlite && !SQLITE_READABLE.contains(&attribute) {
        let error = DbError {
            state: "IM001".into(),
            code: None,
            detail: "driver does not support that attribute".into(),
        };
        return connection.fail(context, error, "PDO::getAttribute");
    }
    let attributes = connection.attributes.borrow();
    let value = match attribute {
        ATTR_ERRMODE => Value::Int(match attributes.errors {
            ErrorMode::Silent => ERRMODE_SILENT,
            ErrorMode::Warning => ERRMODE_WARNING,
            ErrorMode::Exception => ERRMODE_EXCEPTION,
        }),
        ATTR_SERVER_VERSION => Value::from(connection.version.clone()),
        // SQLite's client is its engine; the server drivers' client versions
        // are not known.
        ATTR_CLIENT_VERSION if connection.name == DriverName::Sqlite => {
            Value::from(connection.version.clone())
        }
        SQLITE_ATTR_TRANSACTION_MODE => Value::Int(attributes.transaction_mode),
        ATTR_CASE => Value::Int(match attributes.case {
            ColumnCase::Natural => CASE_NATURAL,
            ColumnCase::Upper => CASE_UPPER,
            ColumnCase::Lower => CASE_LOWER,
        }),
        ATTR_PERSISTENT => Value::Bool(attributes.persistent),
        ATTR_DRIVER_NAME => Value::from(connection.name.name()),
        ATTR_STRINGIFY_FETCHES => Value::Bool(attributes.stringify),
        ATTR_ORACLE_NULLS => Value::Int(attributes.oracle_nulls),
        ATTR_DEFAULT_FETCH_MODE => Value::Int(attributes.fetch.number()),
        ATTR_EMULATE_PREPARES => Value::Bool(attributes.emulate),
        _ => return Err(unsupported("This PDO attribute")),
    };
    Ok(value)
}

#[cfg(test)]
mod tests {
    use phera_extension_api::testing::TestContext;
    use phera_extension_api::{NativeError, Value};

    use super::Attributes;
    use crate::dsn::DriverName;

    fn refusal(attribute: i64) -> String {
        let mut attributes = Attributes::new(DriverName::Mysql);
        let error = attributes
            .set(&mut TestContext::default(), attribute, &Value::from("x"))
            .unwrap_err();
        let NativeError::Unwind(_) = &error else {
            panic!("a PDOException")
        };
        let (_, message, _, _) = error.into_exception_parts().unwrap();
        message
    }

    #[test]
    fn mysql_tls_options_are_refused_as_tls() {
        // SSL_KEY, SSL_CERT, SSL_CA, SSL_CAPATH, SSL_CIPHER, SSL_VERIFY_SERVER_CERT
        for attribute in [1006, 1007, 1008, 1009, 1010, 1013] {
            assert_eq!(
                refusal(attribute),
                "TLS is not supported yet",
                "{attribute}"
            );
        }
        // SERVER_PUBLIC_KEY and LOCAL_INFILE_DIRECTORY are not TLS options.
        for attribute in [1011, 1014] {
            assert_eq!(
                refusal(attribute),
                "This PDO attribute is not supported yet"
            );
        }
    }
}
