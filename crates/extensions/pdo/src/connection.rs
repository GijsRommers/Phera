//! A PDO owns its connection: `$pdo = new PDO($dsn, $user, $password)` awaits the handshake.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args,
};
use tokio::sync::Mutex;

use crate::attributes::{self, AttributeCall, Attributes};
use crate::constants::{
    ATTR_ERRMODE, ATTR_PERSISTENT, PARAM_BOOL, PARAM_INT, PARAM_LOB, PARAM_NULL, PARAM_STR,
    STR_CHAR, STR_NATL,
};
use crate::dsn::{DriverName, Dsn};
use crate::errors::{DbError, ErrorMode, ErrorState, unsupported};
use crate::pool::{self, Lease, Policy};

pub struct Session {
    pub driver: Mutex<Lease>,
    pub name: DriverName,
    pub version: String,
    pub attributes: RefCell<Attributes>,
    pub error: RefCell<ErrorState>,
    pub last_error: RefCell<Option<DbError>>,
    pub transaction: Cell<bool>,
    pub statement_sequence: Cell<u64>,
}

impl Session {
    pub fn fail(
        &self,
        context: &mut dyn Context,
        mut error: DbError,
        method: &str,
    ) -> NativeResult {
        if self.name == DriverName::Sqlite {
            crate::drivers::sqlite::normalize(&mut error, self.attributes.borrow().extended);
        }
        *self.last_error.borrow_mut() = Some(error.clone());
        *self.error.borrow_mut() = ErrorState::Failed(error.clone());
        let mode = self.attributes.borrow().errors;
        error.report(context, mode, method)
    }

    pub fn success(&self) {
        *self.error.borrow_mut() = ErrorState::Success;
    }

    pub fn fail_get_attribute(&self, context: &mut dyn Context, error: DbError) -> NativeResult {
        *self.error.borrow_mut() = ErrorState::Failed(DbError {
            detail: String::new(),
            ..error.clone()
        });
        error.report(context, self.attribute_error_mode(), "PDO::getAttribute")
    }

    pub fn fail_attribute(
        &self,
        context: &mut dyn Context,
        error: DbError,
        call: AttributeCall,
    ) -> NativeResult {
        let actual = self.attributes.borrow().errors;
        if matches!(call, AttributeCall::Construct) && !matches!(actual, ErrorMode::Exception) {
            return Err(crate::errors::exception(
                format!("PDO::__construct(): {}", error.message()),
                None,
                0,
            ));
        }
        self.fail(context, error, "PDO::setAttribute")
    }

    fn attribute_error_mode(&self) -> ErrorMode {
        match self.attributes.borrow().errors {
            ErrorMode::Exception => ErrorMode::Exception,
            _ => ErrorMode::Warning,
        }
    }
}

pub fn session(this: Option<&Rc<Object>>) -> Result<Rc<Session>, NativeError> {
    let object = this.ok_or_else(|| NativeError::error("PDO object is missing"))?;
    object
        .native
        .borrow()
        .as_ref()
        .and_then(|native| native.downcast_ref::<Rc<Session>>())
        .cloned()
        .ok_or_else(|| NativeError::error("PDO object is uninitialized"))
}

pub fn construct<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let object = this.ok_or_else(|| NativeError::error("PDO object is missing"))?;
        if object.native.borrow().is_some() {
            return Err(NativeError::error("PDO object is already initialized"));
        }
        let dsn_text = args::string(&arguments, 0)
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        let mut dsn = Dsn::parse(&dsn_text)?;
        let username = args::string(&arguments, 1)
            .map(|s| s.to_string_lossy())
            .unwrap_or_else(|| dsn.get("user", "").to_owned());
        let password = args::string(&arguments, 2)
            .map(|s| s.to_string_lossy())
            .unwrap_or_else(|| dsn.get("password", "").to_owned());
        let policy = Policy::from_options(args::array(&arguments, 3).map(Rc::as_ref));
        let mut attributes = Attributes::new(dsn.driver);
        if let Some(options) = args::array(&arguments, 3) {
            if let Some(value) = options.get(&ArrayKey::Int(ATTR_PERSISTENT)) {
                use phera_extension_api::value::{is_numeric, to_int};
                attributes.persistent = match &*value.deref() {
                    Value::String(text)
                        if !text.as_bytes().is_empty() && !is_numeric(text.as_bytes()) =>
                    {
                        true
                    }
                    value => to_int(value, context.diagnostics())? != 0,
                };
            }
            if let Some(value) = options.get(&ArrayKey::Int(ATTR_ERRMODE)) {
                attributes.errors =
                    match phera_extension_api::value::to_int(value, context.diagnostics())? {
                        0 => ErrorMode::Silent,
                        1 => ErrorMode::Warning,
                        _ => ErrorMode::Exception,
                    };
            }
            if dsn.driver == DriverName::Sqlite {
                for (key, value) in options.iter() {
                    if let ArrayKey::Int(attribute) = key {
                        attributes::sqlite_pre_open(context, &mut attributes, attribute, value)?;
                    }
                }
            } else {
                dsn.options =
                    crate::connect_options::ConnectOptions::from_php(context, dsn.driver, options)?;
                attributes.autocommit = dsn.options.mysql.autocommit;
            }
        }
        if dsn.driver == DriverName::Sqlite {
            // The driver opens the file with the options' flags and timeout.
            dsn.fields
                .insert("flags".into(), attributes.sqlite_flags.to_string());
            dsn.fields
                .insert("timeout".into(), attributes.timeout.to_string());
        }
        let driver = match pool::acquire(&dsn, &username, &password, policy).await {
            Ok(driver) => driver,
            Err(error) => {
                let code = error.code.unwrap_or(0);
                return Err(crate::errors::exception(
                    format!("SQLSTATE[{}] [{code}] {}", error.state, error.detail),
                    Some(&error),
                    code,
                ));
            }
        };
        let version = driver.version().to_owned();
        let connection = Rc::new(Session {
            driver: Mutex::new(driver),
            name: dsn.driver,
            version,
            attributes: RefCell::new(attributes),
            error: RefCell::new(ErrorState::Unused),
            last_error: RefCell::new(None),
            transaction: Cell::new(false),
            statement_sequence: Cell::new(0),
        });
        if let Some(options) = args::array(&arguments, 3) {
            for (key, value) in options.iter() {
                if let ArrayKey::Int(attribute) = key {
                    attributes::apply(
                        context,
                        &connection,
                        attribute,
                        value,
                        AttributeCall::Construct,
                    )
                    .await?;
                }
            }
        }
        *object.native.borrow_mut() = Some(Box::new(connection));
        Ok(Value::Null)
    })
}

pub fn available(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let mut drivers = PhpArray::new();
    drivers.push(Value::from("mysql"))?;
    drivers.push(Value::from("pgsql"))?;
    drivers.push(Value::from("sqlite"))?;
    Ok(Value::from(drivers))
}

pub fn drivers(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    available(context, None, arguments)
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
        if sql.is_empty() {
            return Err(NativeError::value_error(
                "PDO::exec(): Argument #1 ($statement) must not be empty",
            ));
        }
        let mut driver = session.driver.lock().await;
        let result = driver.exec(&sql).await;
        if let Some(active) = driver.observed_transaction_state() {
            session.transaction.set(active);
        }
        drop(driver);
        match result {
            Ok(rows) => {
                session.success();
                let affected = if rows.columns.is_empty() {
                    rows.affected
                } else {
                    0
                };
                Ok(Value::Int(affected as i64))
            }
            Err(error) => session.fail(context, error, "PDO::exec"),
        }
    })
}

pub fn quote<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let session = session(this.as_ref())?;
        if session.name == DriverName::Sqlite {
            let bytes = args::string(&arguments, 0).map_or(&b""[..], |s| s.as_bytes());
            return Ok(Value::from(crate::drivers::sqlite::quote(bytes)));
        }
        let kind = args::int(&arguments, 1).unwrap_or(PARAM_STR);
        let base = kind & !(STR_CHAR | STR_NATL);
        if !matches!(base, PARAM_NULL | PARAM_INT | PARAM_STR | PARAM_BOOL)
            && !(base == PARAM_LOB && session.name == DriverName::Mysql)
        {
            return Err(unsupported("LOB quoting"));
        }
        let bytes = args::string(&arguments, 0).map_or(&b""[..], |s| s.as_bytes());
        if bytes.contains(&0) && session.name == DriverName::Postgres {
            return Err(unsupported("Quoting PostgreSQL strings containing NUL"));
        }
        if std::str::from_utf8(bytes).is_err() {
            return Err(unsupported("Quoting non-UTF-8 strings"));
        }
        let result = session.driver.lock().await.quote(bytes).await;
        match result {
            Ok(quoted) => {
                let prefix = if session.name != DriverName::Mysql {
                    ""
                } else if base == PARAM_LOB {
                    "_binary"
                } else if kind & STR_CHAR == 0
                    && (kind & STR_NATL != 0 || session.attributes.borrow().national_strings)
                {
                    "N"
                } else {
                    ""
                };
                Ok(Value::from(if prefix.is_empty() {
                    quoted
                } else {
                    format!("{prefix}{quoted}")
                }))
            }
            Err(error) => session.fail(context, error, "PDO::quote"),
        }
    })
}

pub fn last_id<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let session = session(this.as_ref())?;
        let name = args::string(&arguments, 0).map(|s| s.to_string_lossy());
        let result = session.driver.lock().await.last_id(name.as_deref()).await;
        match result {
            Ok(id) => {
                session.success();
                Ok(Value::from(id))
            }
            Err(error) => session.fail(context, error, "PDO::lastInsertId"),
        }
    })
}

pub fn error_code(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(session(this)?.error.borrow().code())
}

pub fn error_info(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    session(this)?.error.borrow().info()
}

/// PHP 8.4's static factory uses exactly the same constructor path.
pub fn connect<'a>(
    context: &'a mut dyn Context,
    _: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let dsn = args::string(&arguments, 0)
            .map(|value| value.to_string_lossy())
            .unwrap_or_default();
        let class = match Dsn::parse_named(&dsn, "PDO::connect")?.driver {
            DriverName::Postgres => "Pdo\\Pgsql",
            DriverName::Mysql => "Pdo\\Mysql",
            DriverName::Sqlite => "Pdo\\Sqlite",
        };
        let object = context.create_object(class, PhpArray::new())?;
        let Value::Object(instance) = &object else {
            return Err(NativeError::error(
                "PDO allocation did not create an object",
            ));
        };
        construct(context, Some(instance.clone()), arguments).await?;
        Ok(object)
    })
}
