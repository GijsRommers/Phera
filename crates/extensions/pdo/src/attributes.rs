//! `$pdo->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT)` changes later failures.

use std::rc::Rc;

use phera_extension_api::value::{NumVal, parse_numeric, to_int};
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value, args};

use crate::connection::{Session, session};
use crate::constants::*;
use crate::dsn::DriverName;
use crate::errors::{DbError, ErrorMode, unsupported};
use crate::fetch::FetchMode;
use crate::statement_class::StatementClass;

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
    pub oracle_nulls: i64,
    pub persistent: bool,
    pub statement_class: StatementClass,
    pub timeout: i64,
    pub sqlite_flags: i64,
    pub extended: bool,
    pub transaction_mode: i64,
    pub autocommit: bool,
    pub mysql_table_names: bool,
    pub mysql_buffered: bool,
    pub national_strings: bool,
    pub pgsql_disable_prepares: bool,
}

impl Attributes {
    pub fn new(driver: DriverName) -> Self {
        Self {
            errors: ErrorMode::Exception,
            fetch: FetchMode::Both,
            emulate: driver == DriverName::Mysql,
            stringify: false,
            case: ColumnCase::Natural,
            oracle_nulls: 0,
            persistent: false,
            statement_class: StatementClass::default(),
            timeout: 60,
            sqlite_flags: SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE,
            extended: false,
            transaction_mode: 0,
            autocommit: true,
            mysql_table_names: false,
            mysql_buffered: true,
            national_strings: false,
            pgsql_disable_prepares: false,
        }
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

fn boolean(value: &Value) -> Result<bool, NativeError> {
    match &*value.deref() {
        Value::Bool(value) => Ok(*value),
        Value::Int(value) => Ok(*value != 0),
        _ => Err(NativeError::type_error(format!(
            "Attribute value must be of type bool for selected attribute, {} given",
            value.type_name()
        ))),
    }
}

/// PDO applies core and mutable driver options after establishing the connection.
#[derive(Clone, Copy)]
pub enum AttributeCall {
    Construct,
    Set,
}

impl AttributeCall {
    pub fn argument(self) -> &'static str {
        match self {
            Self::Construct => "PDO::__construct(): Argument #3 ($password)",
            Self::Set => "PDO::setAttribute(): Argument #2 ($value)",
        }
    }
}

pub async fn apply(
    context: &mut dyn Context,
    connection: &Rc<Session>,
    attribute: i64,
    value: &Value,
    call: AttributeCall,
) -> NativeResult {
    let driver = connection.name;
    match (driver, attribute) {
        (_, ATTR_STATEMENT_CLASS) => {
            if connection.attributes.borrow().persistent {
                let error = DbError::general(
                    "PDO::ATTR_STATEMENT_CLASS cannot be used with persistent PDO instances",
                );
                return connection.fail_attribute(context, error, call);
            }
            let class = StatementClass::parse(context, value, call).await?;
            connection.attributes.borrow_mut().statement_class = class;
            return Ok(Value::Bool(true));
        }
        (DriverName::Sqlite, ATTR_TIMEOUT) => {
            let seconds = integer(value)?;
            let milliseconds = ((seconds.wrapping_mul(1000) as i32).max(0)) as u64;
            if let Err(error) = connection
                .driver
                .lock()
                .await
                .sqlite_timeout(milliseconds)
                .await
            {
                return connection.fail(context, error, "PDO::setAttribute");
            }
        }
        (DriverName::Mysql, ATTR_AUTOCOMMIT) => {
            let enabled = boolean(value)?;
            if connection.attributes.borrow().autocommit != enabled {
                connection.attributes.borrow_mut().autocommit = enabled;
                let result = {
                    let mut lease = connection.driver.lock().await;
                    let result = lease.mysql_autocommit(enabled).await;
                    if let Some(active) = lease.observed_transaction_state() {
                        connection.transaction.set(active);
                    }
                    result
                };
                if let Err(error) = result {
                    return connection.fail(context, error, "PDO::setAttribute");
                }
            }
        }
        _ => {}
    }
    change(
        &mut connection.attributes.borrow_mut(),
        driver,
        attribute,
        value,
        call,
    )
}

fn change(
    attrs: &mut Attributes,
    driver: DriverName,
    attribute: i64,
    value: &Value,
    call: AttributeCall,
) -> NativeResult {
    match (driver, attribute) {
        (_, ATTR_ERRMODE) => {
            attrs.errors = match integer(value)? {
                ERRMODE_SILENT => ErrorMode::Silent,
                ERRMODE_WARNING => ErrorMode::Warning,
                ERRMODE_EXCEPTION => ErrorMode::Exception,
                _ => {
                    return Err(invalid(
                        call,
                        "Error mode must be one of the PDO::ERRMODE_* constants",
                    ));
                }
            }
        }
        (_, ATTR_CASE) => {
            attrs.case = match integer(value)? {
                CASE_NATURAL => ColumnCase::Natural,
                CASE_UPPER => ColumnCase::Upper,
                CASE_LOWER => ColumnCase::Lower,
                _ => {
                    return Err(invalid(
                        call,
                        "Case folding mode must be one of the PDO::CASE_* constants",
                    ));
                }
            }
        }
        (_, ATTR_ORACLE_NULLS) => attrs.oracle_nulls = integer(value)?,
        (_, ATTR_STRINGIFY_FETCHES) => attrs.stringify = boolean(value)?,
        (_, ATTR_DEFAULT_FETCH_MODE) => {
            let number = match &*value.deref() {
                Value::Array(items) => {
                    if matches!(
                        items.get(&phera_extension_api::ArrayKey::Int(0)),
                        Some(Value::Int(FETCH_CLASS | 9))
                    ) {
                        return Err(invalid(
                            call,
                            "PDO::FETCH_INTO and PDO::FETCH_CLASS cannot be set as the default fetch mode",
                        ));
                    }
                    i64::from(!items.is_empty())
                }
                value => integer(value)?,
            };
            if number == FETCH_DEFAULT {
                return Err(invalid(
                    call,
                    "Fetch mode must be a bitmask of PDO::FETCH_* constants",
                ));
            }
            attrs.fetch = FetchMode::parse(number, attrs.fetch)?;
        }
        (DriverName::Sqlite, ATTR_TIMEOUT) => attrs.timeout = integer(value)?,
        (DriverName::Sqlite, SQLITE_ATTR_EXTENDED_RESULT_CODES) => {
            attrs.extended = integer(value)? != 0
        }
        (DriverName::Sqlite, SQLITE_ATTR_TRANSACTION_MODE) => {
            let mode = integer(value)?;
            if !(0..=2).contains(&mode) {
                return Ok(Value::Bool(false));
            }
            attrs.transaction_mode = mode;
        }
        (DriverName::Mysql | DriverName::Postgres, ATTR_EMULATE_PREPARES) => {
            attrs.emulate = boolean(value)?
        }
        (DriverName::Mysql, ATTR_AUTOCOMMIT) => {}
        (DriverName::Mysql, ATTR_DEFAULT_STR_PARAM) => {
            attrs.national_strings = integer(value)? == STR_NATL
        }
        (DriverName::Mysql, ATTR_FETCH_TABLE_NAMES) => attrs.mysql_table_names = boolean(value)?,
        (DriverName::Mysql, MYSQL_BUFFERED) => attrs.mysql_buffered = boolean(value)?,
        (DriverName::Postgres, PGSQL_DISABLE_PREPARES) => {
            attrs.pgsql_disable_prepares = boolean(value)?
        }
        (DriverName::Postgres, ATTR_PREFETCH) if boolean(value)? => {}
        (DriverName::Postgres, ATTR_PREFETCH) => {
            return Err(unsupported("Lazy PostgreSQL fetches"));
        }
        _ => return Ok(Value::Bool(false)),
    }
    Ok(Value::Bool(true))
}

fn invalid(call: AttributeCall, message: &str) -> NativeError {
    NativeError::value_error(format!("{} {message}", call.argument()))
}

pub fn set<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let connection = session(this.as_ref())?;
        connection.success();
        let attribute = args::int(&arguments, 0).unwrap_or_default();
        apply(
            context,
            &connection,
            attribute,
            args::value(&arguments, 1),
            AttributeCall::Set,
        )
        .await
    })
}

pub fn get<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let connection = session(this.as_ref())?;
        connection.success();
        let attribute = args::int(&arguments, 0).unwrap_or_default();
        if connection.name != DriverName::Sqlite
            && matches!(attribute, ATTR_CONNECTION_STATUS | ATTR_SERVER_INFO)
        {
            let result = connection
                .driver
                .lock()
                .await
                .server_attribute(attribute)
                .await;
            return match result {
                Ok(text) => Ok(Value::from(text)),
                Err(error) => connection.fail(context, error, "PDO::getAttribute"),
            };
        }
        let attrs = connection.attributes.borrow();
        let value = match (connection.name, attribute) {
            (_, ATTR_ERRMODE) => Value::Int(match attrs.errors {
                ErrorMode::Silent => ERRMODE_SILENT,
                ErrorMode::Warning => ERRMODE_WARNING,
                ErrorMode::Exception => ERRMODE_EXCEPTION,
            }),
            (_, ATTR_SERVER_VERSION) | (DriverName::Sqlite, ATTR_CLIENT_VERSION) => {
                Value::from(connection.version.clone())
            }
            (DriverName::Mysql, ATTR_CLIENT_VERSION) => Value::from("mysql_async 0.37.1"),
            (DriverName::Postgres, ATTR_CLIENT_VERSION) => Value::from("tokio-postgres 0.7.18"),
            (_, ATTR_CASE) => Value::Int(match attrs.case {
                ColumnCase::Natural => CASE_NATURAL,
                ColumnCase::Upper => CASE_UPPER,
                ColumnCase::Lower => CASE_LOWER,
            }),
            (_, ATTR_PERSISTENT) => Value::Bool(attrs.persistent),
            (_, ATTR_DRIVER_NAME) => Value::from(connection.name.name()),
            (_, ATTR_STRINGIFY_FETCHES) => Value::Bool(attrs.stringify),
            (_, ATTR_ORACLE_NULLS) => Value::Int(attrs.oracle_nulls),
            (_, ATTR_STATEMENT_CLASS) => attrs.statement_class.as_attribute()?,
            (_, ATTR_DEFAULT_FETCH_MODE) => Value::Int(attrs.fetch.number()),
            (DriverName::Sqlite, SQLITE_ATTR_TRANSACTION_MODE) => {
                Value::Int(attrs.transaction_mode)
            }
            (DriverName::Mysql | DriverName::Postgres, ATTR_EMULATE_PREPARES) => {
                Value::Bool(attrs.emulate)
            }
            (DriverName::Mysql, ATTR_AUTOCOMMIT) => Value::Bool(attrs.autocommit),
            (DriverName::Mysql, ATTR_DEFAULT_STR_PARAM) => Value::Int(if attrs.national_strings {
                STR_NATL
            } else {
                STR_CHAR
            }),
            (DriverName::Mysql, ATTR_FETCH_TABLE_NAMES) => Value::Bool(attrs.mysql_table_names),
            (DriverName::Mysql, MYSQL_BUFFERED) => Value::Bool(attrs.mysql_buffered),
            (DriverName::Mysql, MYSQL_LOCAL_INFILE) => Value::Bool(false),
            (DriverName::Mysql, MYSQL_LOCAL_INFILE_DIRECTORY) => Value::Null,
            (DriverName::Postgres, PGSQL_DISABLE_PREPARES) => {
                Value::Bool(attrs.pgsql_disable_prepares)
            }
            _ => {
                drop(attrs);
                let error = DbError {
                    state: "IM001".into(),
                    code: None,
                    detail: "driver does not support that attribute".into(),
                };
                return connection.fail_get_attribute(context, error);
            }
        };
        Ok(value)
    })
}

pub fn sqlite_pre_open(
    context: &mut dyn Context,
    attrs: &mut Attributes,
    attribute: i64,
    value: &Value,
) -> Result<(), NativeError> {
    match attribute {
        ATTR_TIMEOUT => attrs.timeout = to_int(value, context.diagnostics())?,
        SQLITE_ATTR_OPEN_FLAGS => attrs.sqlite_flags = to_int(value, context.diagnostics())?,
        _ => {}
    }
    Ok(())
}
