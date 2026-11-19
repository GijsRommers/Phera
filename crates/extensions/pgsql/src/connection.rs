//! `pg_connect('host=127.0.0.1 dbname=app')` makes a request-local connection object.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use phera_db_drivers::drivers::{
    Driver, connection_error,
    postgres::{NativeResult as Rows, Postgres},
};
use phera_db_drivers::dsn::{DriverName, Dsn, postgres};
use phera_db_drivers::errors::DbError;
use phera_db_drivers::pool::{self, Policy};
use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args,
};
use phera_io_pool::{Lease, Operation};
use tokio::task::JoinHandle;

const DEFAULT: &str = "pgsql.default";
const CACHE: &str = "pgsql.connections";

pub type Handle = Rc<RefCell<Connection>>;
pub type Pending = JoinHandle<Result<Vec<Rows>, DbError>>;

pub struct Connection {
    pub lease: Option<Lease<Driver>>,
    pub dsn: Dsn,
    pub busy: Option<Pending>,
    pub results: VecDeque<Rows>,
    pub last_error: String,
    pub server_version: String,
    pub pid: i32,
    pub encoding: String,
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(pending) = &self.busy {
            pending.abort();
        }
    }
}

impl Connection {
    pub fn lease(&self) -> Result<Lease<Driver>, NativeError> {
        self.lease
            .clone()
            .ok_or_else(|| NativeError::error("PostgreSQL connection has already been closed"))
    }
}

pub fn argument(
    context: &mut dyn Context,
    value: &Value,
    call: &str,
) -> Result<Handle, NativeError> {
    if let Value::Object(object) = &*value.deref() {
        return object_handle(object);
    }
    let default = context.extension_value(DEFAULT);
    let Some(Value::Object(object)) = default else {
        return Err(NativeError::error("No PostgreSQL connection opened yet"));
    };
    context.deprecated(format!(
        "{call}(): Automatic fetching of PostgreSQL connection is deprecated"
    ));
    object_handle(&object)
}

pub fn object_handle(object: &Object) -> Result<Handle, NativeError> {
    object
        .native
        .borrow()
        .as_ref()
        .and_then(|native| native.downcast_ref::<Handle>())
        .cloned()
        .ok_or_else(|| NativeError::error("PostgreSQL connection has already been closed"))
}

pub async fn run<T, F>(lease: &Lease<Driver>, operation: F) -> Result<T, DbError>
where
    T: Send + 'static,
    F: for<'a> FnOnce(&'a mut Postgres) -> Operation<'a, Result<T, DbError>> + Send + 'static,
{
    pool::run(lease, |driver| match driver {
        Driver::Postgres(postgres) => operation(postgres),
        _ => Box::pin(async { Err(connection_error("Mismatched database driver")) }),
    })
    .await
}

pub fn connect<const PERSISTENT: bool>(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let connection_string =
            args::string(&arguments, 0).map_or_else(String::new, |s| s.to_string_lossy());
        let call = if PERSISTENT {
            "pg_pconnect"
        } else {
            "pg_connect"
        };
        let flags = args::int(&arguments, 1).unwrap_or_default();
        let key = ArrayKey::str(format!("{call}\0{connection_string}"));
        let mut cache = match context.extension_value(CACHE) {
            Some(Value::Array(array)) => (*array).clone(),
            _ => PhpArray::new(),
        };
        if flags & 2 == 0 {
            if let Some(Value::Object(object)) = cache.get(&key) {
                if object_handle(object)?.borrow().lease.is_some() {
                    let value = Value::Object(object.clone());
                    context.set_extension_value(DEFAULT, value.clone());
                    return Ok(value);
                }
            }
        }
        let mut dsn = match parse(&connection_string) {
            Ok(dsn) => dsn,
            Err(error) => return fail_connect(context, call, &error),
        };
        let user = dsn
            .fields
            .get("user")
            .cloned()
            .or_else(|| std::env::var("USER").ok())
            .unwrap_or_default();
        let password = dsn.get("password", "").to_owned();
        dsn.fields
            .entry("dbname".into())
            .or_insert_with(|| user.clone());
        dsn.fields
            .entry("port".into())
            .or_insert_with(|| "5432".into());
        let policy = if PERSISTENT
            && context
                .ini_get(b"pgsql.allow_persistent")
                .is_none_or(|value| value.as_bytes() != b"0")
            && flags & 2 == 0
        {
            Policy::Persistent
        } else {
            Policy::Unpooled
        };
        let lease = match pool::acquire(&dsn, &user, &password, policy).await {
            Ok(lease) => lease,
            Err(error) => return fail_connect(context, call, &error.detail),
        };
        let (server_version, pid) = run(&lease, |pg| {
            Box::pin(async { Ok((pg.version.clone(), pg.pid())) })
        })
        .await
        .map_err(|error| NativeError::error(error.detail))?;
        let value = context.create_object("PgSql\\Connection", PhpArray::new())?;
        let mut dsn = dsn;
        dsn.fields.remove("password");
        if let Value::Object(object) = &value {
            let state = Rc::new(RefCell::new(Connection {
                lease: Some(lease),
                dsn,
                busy: None,
                results: VecDeque::new(),
                last_error: String::new(),
                server_version,
                pid,
                encoding: "UTF8".into(),
            }));
            *object.native.borrow_mut() = Some(Box::new(state));
        }
        if flags & 2 == 0 {
            cache.insert(key, value.clone());
            context.set_extension_value(CACHE, Value::from(cache));
        }
        context.set_extension_value(DEFAULT, value.clone());
        Ok(value)
    })
}

fn fail_connect(context: &mut dyn Context, call: &str, error: &str) -> NativeResult {
    context.warning(format!(
        "{call}(): Unable to connect to PostgreSQL server: {error}"
    ));
    Ok(Value::Bool(false))
}

fn parse(source: &str) -> Result<Dsn, String> {
    let mut fields = if source.starts_with("postgres://") || source.starts_with("postgresql://") {
        phera_db_drivers::dsn::postgres::uri_fields(source)?
    } else {
        postgres::fields(source, false)?
    };
    for (field, variable) in [
        ("host", "PGHOST"),
        ("port", "PGPORT"),
        ("dbname", "PGDATABASE"),
        ("user", "PGUSER"),
        ("password", "PGPASSWORD"),
        ("options", "PGOPTIONS"),
        ("sslmode", "PGSSLMODE"),
    ] {
        if let Ok(value) = std::env::var(variable) {
            fields.entry(field.into()).or_insert(value);
        }
    }
    let timeout = fields
        .get("connect_timeout")
        .and_then(|value| value.parse().ok());
    Ok(Dsn {
        driver: DriverName::Postgres,
        fields,
        options: phera_db_drivers::connect_options::ConnectOptions {
            postgres_native: true,
            timeout,
            ..Default::default()
        },
    })
}

pub fn close(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let handle = argument(context, args::value(arguments, 0), "pg_close")?;
    let mut state = handle.borrow_mut();
    state.lease()?;
    if let Some(pending) = state.busy.take() {
        pending.abort();
    }
    state.lease = None;
    state.results.clear();
    if context
        .extension_value(DEFAULT)
        .as_ref()
        .and_then(|value| {
            let Value::Object(object) = value else {
                return None;
            };
            object_handle(object).ok()
        })
        .is_some_and(|other| Rc::ptr_eq(&other, &handle))
    {
        context.set_extension_value(DEFAULT, Value::Null);
    }
    Ok(Value::Bool(true))
}

pub fn last_error(state: &Connection) -> NativeResult {
    Ok(Value::from(state.last_error.clone()))
}
