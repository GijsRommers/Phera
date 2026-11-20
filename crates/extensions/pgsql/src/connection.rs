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
use phera_db_drivers::pool::{self, BackgroundOperation, Lease, Operation, Policy};
use phera_db_drivers::session_log::SessionLog;
use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args,
};
use tokio::task::JoinHandle;

const DEFAULT: &str = "pgsql.default";
const CACHE: &str = "pgsql.connections";

pub type Handle = Rc<RefCell<Connection>>;
pub type Pending = JoinHandle<Result<Vec<Rows>, DbError>>;

pub struct Connection {
    pub lease: Option<Lease>,
    pub dsn: Dsn,
    pub busy: Option<Pending>,
    pub results: VecDeque<Rows>,
    pub last_error: String,
    pub server_version: String,
    pub pid: i32,
    pub encoding: String,
    credentials: Credentials,
    /// How `pg_connect()` borrowed the connection.
    policy: Policy,
    /// A fork's copy of a connection the boot made: it borrows its own
    /// session on first use (see [`fork_connection`]).
    reconnect: bool,
    /// What the script changed about the session, for a fork to replay.
    pub log: SessionLog,
}

#[derive(Clone)]
struct Credentials {
    user: String,
    password: String,
}

impl Connection {
    /// Fails if the connection was closed. A fork's copy that has not
    /// connected yet is open.
    pub fn ensure_open(&self) -> Result<(), NativeError> {
        if self.lease.is_none() && !self.reconnect {
            return Err(NativeError::error(
                "PostgreSQL connection has already been closed",
            ));
        }
        Ok(())
    }
}

/// The connection's lease; a fork's copy of a connection the boot made
/// connects now, and replays what the boot changed about its session.
pub async fn lease(handle: &Handle) -> Result<Lease, NativeError> {
    let (dsn, credentials, policy, statements) = {
        let state = handle.borrow();
        state.ensure_open()?;
        if let Some(lease) = &state.lease {
            return Ok(lease.clone());
        }
        (
            state.dsn.clone(),
            state.credentials.clone(),
            state.policy,
            state.log.statements().to_vec(),
        )
    };
    let unable = |error: DbError| {
        NativeError::error(format!(
            "Unable to connect to PostgreSQL server: {}",
            error.detail
        ))
    };
    let lease = pool::acquire(&dsn, &credentials.user, &credentials.password, policy)
        .await
        .map_err(unable)?;
    let pid = run(&lease, move |postgres| {
        Box::pin(async move {
            for statement in statements {
                postgres
                    .native_query(&String::from_utf8_lossy(&statement))
                    .await?;
            }
            Ok(postgres.pid())
        })
    })
    .await
    .map_err(unable)?;
    let mut state = handle.borrow_mut();
    state.pid = pid;
    state.reconnect = false;
    state.lease = Some(lease.clone());
    Ok(lease)
}

/// The fork hook of `PgSql\Connection`: each fork's copy borrows its own
/// session on first use, from the shared pool (which resets a session
/// before lending it again), and replays the boot's `SET` statements.
///
/// ```php
/// $db = pg_connect('host=db dbname=shop user=app');
/// pg_query($db, "SET TIME ZONE 'UTC'");
/// Phera\snapshot();
/// pg_query($db, 'SELECT now()');   // each request: its own session, UTC
/// ```
pub fn fork_connection(
    _object: &Object,
    state: &dyn std::any::Any,
    _fork: &mut phera_extension_api::value::Fork<'_>,
) -> Result<Box<dyn std::any::Any>, String> {
    const ADVICE: &str = "; finish it before Phera\\snapshot(), or connect after the call";
    let handle = state
        .downcast_ref::<Handle>()
        .ok_or("its built-in state is not initialized")?;
    let state = handle.borrow();
    if state.busy.is_some() || !state.results.is_empty() {
        return Err(format!("a query result on it was not read yet{ADVICE}"));
    }
    if let Some(reason) = state.log.unreplayable() {
        return Err(format!(
            "{reason}, which a new session would not have{ADVICE}"
        ));
    }
    if let Some(lease) = &state.lease {
        let transaction = lease
            .try_inspect(|driver| match driver {
                Driver::Postgres(postgres) => postgres.transaction_status(),
                _ => 0,
            })
            .ok_or_else(|| format!("it is busy with an operation{ADVICE}"))?;
        // PGSQL_TRANSACTION_IDLE is 0; anything else is inside a transaction.
        if transaction != 0 {
            return Err(format!("a transaction is open on it{ADVICE}"));
        }
    }
    let copy = Connection {
        lease: None,
        dsn: state.dsn.clone(),
        busy: None,
        results: VecDeque::new(),
        last_error: state.last_error.clone(),
        server_version: state.server_version.clone(),
        pid: state.pid,
        encoding: state.encoding.clone(),
        credentials: state.credentials.clone(),
        policy: match state.policy {
            Policy::Persistent => Policy::Persistent,
            Policy::Pooled | Policy::Unpooled => Policy::Pooled,
        },
        reconnect: state.lease.is_some() || state.reconnect,
        log: state.log.forked(),
    };
    Ok(Box::new(Rc::new(RefCell::new(copy))) as Box<dyn std::any::Any>)
}

pub fn argument(
    context: &mut dyn Context,
    value: &Value,
    call: &str,
) -> Result<Handle, NativeError> {
    if let Value::Object(object) = &*value.deref() {
        return object_handle(object);
    }
    let Some(state) = default_handle(context)? else {
        return Err(NativeError::error("No PostgreSQL connection opened yet"));
    };
    context.deprecated(format!(
        "{call}(): Automatic fetching of PostgreSQL connection is deprecated"
    ));
    Ok(state)
}

pub(crate) fn default_handle(context: &dyn Context) -> Result<Option<Handle>, NativeError> {
    match context.extension_value(DEFAULT) {
        Some(Value::Object(object)) => object_handle(&object).map(Some),
        _ => Ok(None),
    }
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

pub async fn run<T, F>(lease: &Lease, operation: F) -> Result<T, DbError>
where
    F: for<'a> FnOnce(&'a mut Postgres) -> Operation<'a, Result<T, DbError>>,
{
    lease
        .run(async move |driver| match driver {
            Driver::Postgres(postgres) => operation(postgres).await,
            _ => Err(connection_error("Mismatched database driver")),
        })
        .await
}

pub async fn run_simple(lease: &Lease, sql: String) -> Result<Vec<Rows>, DbError> {
    run(lease, move |driver| {
        Box::pin(async move { driver.native_query(&sql).await })
    })
    .await
}

/// A pg_send_* operation can progress even when PHP immediately polls for it.
pub fn start<F>(lease: &Lease, operation: F) -> Result<Pending, DbError>
where
    F: for<'a> FnOnce(&'a mut Postgres) -> BackgroundOperation<'a, Result<Vec<Rows>, DbError>>
        + Send
        + 'static,
{
    lease.start(|driver| match driver {
        Driver::Postgres(postgres) => operation(postgres),
        _ => Box::pin(async { Err(connection_error("Mismatched database driver")) }),
    })
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
        if flags & 2 == 0
            && let Some(Value::Object(object)) = cache.get(&key)
            && object_handle(object)?.borrow().ensure_open().is_ok()
        {
            let value = Value::Object(object.clone());
            context.set_extension_value(DEFAULT, value.clone());
            return Ok(value);
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
                credentials: Credentials { user, password },
                policy,
                reconnect: false,
                log: SessionLog::new(context.is_snapshot_boot()),
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

pub fn reset(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let state = argument(context, args::value(&arguments, 0), "pg_connection_reset")?;
        let lease = lease(&state).await?;
        let (dsn, user, password) = {
            let mut state = state.borrow_mut();
            state.busy = None;
            state.results.clear();
            state.last_error.clear();
            (
                state.dsn.clone(),
                state.credentials.user.clone(),
                state.credentials.password.clone(),
            )
        };
        let response = run(&lease, move |driver| {
            Box::pin(async move {
                driver.reconnect(&dsn, &user, &password).await?;
                Ok((driver.pid(), driver.version.clone()))
            })
        })
        .await;
        let mut state = state.borrow_mut();
        match response {
            Ok((pid, version)) => {
                state.pid = pid;
                state.server_version = version;
                state.encoding = "UTF8".into();
                state.log = SessionLog::new(state.log.is_recording());
                Ok(Value::Bool(true))
            }
            Err(error) => {
                state.last_error = error.detail;
                Ok(Value::Bool(false))
            }
        }
    })
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
    state.ensure_open()?;
    state.busy = None;
    state.lease = None;
    state.reconnect = false;
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
