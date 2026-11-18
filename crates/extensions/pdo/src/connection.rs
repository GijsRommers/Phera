//! A PDO owns its connection: `$pdo = new PDO($dsn, $user, $password)` awaits the handshake.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args,
};
use tokio::sync::Mutex;

use crate::attributes::Attributes;
use crate::constants::{PARAM_BOOL, PARAM_INT, PARAM_NULL, PARAM_STR};
use crate::dsn::{DriverName, Dsn};
use crate::errors::{DbError, ErrorState, unsupported};
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
            for (key, value) in options.iter() {
                if let ArrayKey::Int(attribute) = key
                    && attribute != pool::PHERA_ATTR_POOL
                    && crate::attributes::constructor_accepts(dsn.driver, attribute)
                {
                    attributes.set(context, attribute, value)?;
                }
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
        *object.native.borrow_mut() = Some(Box::new(Rc::new(Session {
            driver: Mutex::new(driver),
            name: dsn.driver,
            version,
            attributes: RefCell::new(attributes),
            error: RefCell::new(ErrorState::Unused),
            last_error: RefCell::new(None),
            transaction: Cell::new(false),
            statement_sequence: Cell::new(0),
        })));
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
        if !matches!(kind, PARAM_NULL | PARAM_INT | PARAM_STR | PARAM_BOOL) {
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
            Ok(quoted) => Ok(Value::from(quoted)),
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
