//! `$pdo->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_SILENT)` changes later failures.

mod read;

pub use read::get;

use std::rc::Rc;

use phera_extension_api::value::{NumVal, parse_numeric, to_int};
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value, args};

use crate::connection::{Session, session};
use crate::constants::*;
use crate::dsn::DriverName;
use crate::errors::{DbError, ErrorMode};
use crate::fetch::FetchMode;
use crate::statement_class::StatementClass;

#[derive(Clone, Copy)]
pub enum ColumnCase {
    Natural,
    Upper,
    Lower,
}

#[derive(Clone, Copy)]
pub enum NullHandling {
    Natural,
    EmptyString,
    ToString,
}

impl NullHandling {
    pub fn number(self) -> i64 {
        match self {
            Self::Natural => 0,
            Self::EmptyString => 1,
            Self::ToString => 2,
        }
    }
}

#[derive(Clone)]
pub struct Attributes {
    pub errors: ErrorMode,
    pub fetch: FetchMode,
    pub fetch_flags: i64,
    pub emulate: bool,
    pub stringify: bool,
    pub case: ColumnCase,
    pub oracle_nulls: NullHandling,
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
    pub pgsql_prefetch: bool,
}

impl Attributes {
    pub fn new(driver: DriverName) -> Self {
        Self {
            errors: ErrorMode::Exception,
            fetch: FetchMode::Both,
            fetch_flags: 0,
            emulate: driver == DriverName::Mysql,
            stringify: false,
            case: ColumnCase::Natural,
            oracle_nulls: NullHandling::Natural,
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
            pgsql_prefetch: true,
        }
    }
}

pub(crate) fn integer(value: &Value) -> Result<i64, NativeError> {
    match &*value.deref() {
        Value::Int(value) => return Ok(*value),
        Value::Bool(value) => return Ok(i64::from(*value)),
        Value::InternedStr(value) => {
            let parsed = parse_numeric(value.as_bytes());
            if parsed.exact
                && let Some(NumVal::Int(value)) = parsed.value
            {
                return Ok(value);
            }
        }
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
    Connect,
    Set,
}

impl AttributeCall {
    pub fn argument(self) -> &'static str {
        match self {
            Self::Construct => "PDO::__construct(): Argument #4 ($options)",
            Self::Connect => "PDO::connect(): Argument #4 ($options)",
            Self::Set => "PDO::setAttribute(): Argument #2 ($value)",
        }
    }

    pub fn method(self) -> &'static str {
        match self {
            Self::Construct => "PDO::__construct",
            Self::Connect => "PDO::connect",
            Self::Set => "PDO::setAttribute",
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
            if let Err(error) = connection.driver.sqlite_timeout(milliseconds).await {
                return connection.fail(context, error, "PDO::setAttribute");
            }
        }
        (DriverName::Mysql, ATTR_AUTOCOMMIT) => {
            let enabled = boolean(value)?;
            if connection.attributes.borrow().autocommit != enabled {
                connection.attributes.borrow_mut().autocommit = enabled;
                let result = {
                    let lease = &connection.driver;
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
        (_, ATTR_ORACLE_NULLS) => {
            attrs.oracle_nulls = match integer(value)? {
                0 => NullHandling::Natural,
                1 => NullHandling::EmptyString,
                2 => NullHandling::ToString,
                _ => {
                    return Err(invalid(
                        call,
                        "PDO::ATTR_ORACLE_NULLS must be one of the PDO::NULL_* constants",
                    ));
                }
            };
        }
        (_, ATTR_STRINGIFY_FETCHES) => attrs.stringify = boolean(value)?,
        (_, ATTR_DEFAULT_FETCH_MODE) => {
            let number = match &*value.deref() {
                Value::Array(items) => {
                    if matches!(
                        items.get(&phera_extension_api::ArrayKey::Int(0)),
                        Some(Value::Int(FETCH_CLASS | FETCH_INTO))
                    ) {
                        return Err(invalid(
                            call,
                            "PDO::FETCH_INTO and PDO::FETCH_CLASS cannot be set as \
                             the default fetch mode",
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
            let number = i64::from(number as u16);
            attrs.fetch_flags = number & FETCH_GROUPING;
            attrs.fetch = FetchMode::default_attribute(number & !FETCH_GROUPING);
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
        (DriverName::Postgres, ATTR_PREFETCH) => attrs.pgsql_prefetch = boolean(value)?,
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
