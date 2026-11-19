//! `mysqli_init()` creates a handle, then `mysqli_real_connect($db, ...)` connects it.

pub mod commands;
pub mod escape;
pub mod properties;
pub mod query;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use phera_db_drivers::connect_options::ConnectOptions;
use phera_db_drivers::drivers::mysql::{Mysql, Outcome, Reply};
use phera_db_drivers::drivers::{Driver, connection_error};
use phera_db_drivers::dsn::{DriverName, Dsn};
use phera_db_drivers::errors::DbError;
use phera_db_drivers::pool::{self, Policy};
use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args,
};
use phera_io_pool::{Lease, Operation};

use crate::{errors, options};

pub type Handle = Rc<RefCell<Connection>>;

#[derive(Default)]
pub struct Connection {
    status: Status,
    pub options: ConnectOptions,
    pub charset: Option<String>,
    pub native_numbers: bool,
    pub error: Option<DbError>,
    pub reply: Reply,
    pub field_count: usize,
    pub thread_id: u32,
    pub version: String,
    pub host_info: String,
    pub backslashes: bool,
    pub pending: Option<query::Pending>,
    pub more: VecDeque<Result<Outcome, DbError>>,
}

#[derive(Default)]
enum Status {
    #[default]
    Initialized,
    Open(Lease<Driver>),
    Closed,
}

impl Connection {
    pub fn connected(&self) -> bool {
        matches!(self.status, Status::Open(_))
    }

    pub fn lease(&self) -> Result<Lease<Driver>, NativeError> {
        match &self.status {
            Status::Open(lease) => Ok(lease.clone()),
            Status::Initialized => {
                Err(NativeError::error("mysqli object is not fully initialized"))
            }
            Status::Closed => Err(NativeError::error("mysqli object is already closed")),
        }
    }

    pub fn ensure_not_closed(&self) -> Result<(), NativeError> {
        if matches!(self.status, Status::Closed) {
            return Err(NativeError::error("mysqli object is already closed"));
        }
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), NativeError> {
        self.ensure_not_closed()?;
        self.pending = None;
        self.more.clear();
        self.status = Status::Closed;
        Ok(())
    }
}

pub fn initialize(object: &Object) -> Handle {
    let mut native = object.native.borrow_mut();
    if let Some(state) = native
        .as_ref()
        .and_then(|state| state.downcast_ref::<Handle>())
    {
        return state.clone();
    }
    let state = Rc::new(RefCell::new(Connection::default()));
    *native = Some(Box::new(state.clone()));
    state
}

pub fn handle(object: Option<&Rc<Object>>) -> Result<Handle, NativeError> {
    let object = object.ok_or_else(|| NativeError::error("mysqli object is missing"))?;
    let state = initialize(object);
    state.borrow().ensure_not_closed()?;
    Ok(state)
}

pub fn init(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    let value = context.create_object("mysqli", PhpArray::new())?;
    if let Value::Object(object) = &value {
        initialize(object);
    }
    Ok(value)
}

pub async fn run<T, F>(lease: &Lease<Driver>, operation: F) -> Result<T, DbError>
where
    T: Send + 'static,
    F: for<'a> FnOnce(&'a mut Mysql) -> Operation<'a, Result<T, DbError>> + Send + 'static,
{
    pool::run(lease, |driver| match driver {
        Driver::Mysql(mysql) => operation(mysql),
        _ => Box::pin(async { Err(connection_error("Mismatched database driver")) }),
    })
    .await
}

pub fn construct<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = handle(this.as_ref())?;
        if !arguments.is_empty() && arguments.iter().any(|v| !matches!(v, Value::Null)) {
            connect(context, state, &arguments, "mysqli::__construct").await?;
        }
        Ok(Value::Null)
    })
}

pub fn connect_function(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let value = init(context, &[])?;
        let state = handle(args::object(std::slice::from_ref(&value), 0))?;
        match connect(context, state, &arguments, "mysqli_connect").await? {
            Value::Bool(true) => Ok(value),
            value => Ok(value),
        }
    })
}

pub async fn connect(
    context: &mut dyn Context,
    state: Handle,
    args: &[Value],
    call: &str,
) -> NativeResult {
    if matches!(state.borrow().status, Status::Open(_)) {
        return Err(NativeError::error("mysqli object is already connected"));
    }
    let default = |name: &[u8]| context.ini_get(name).map(|text| text.to_string_lossy());
    let text = |index| args::string(args, index).map(|text| text.to_string_lossy());
    let host = text(0)
        .filter(|host| !host.is_empty())
        .or_else(|| default(b"mysqli.default_host"))
        .unwrap_or_else(|| "localhost".into());
    let (host, persistent) = host
        .strip_prefix("p:")
        .map_or((host.as_str(), false), |host| {
            let allowed = default(b"mysqli.allow_persistent").as_deref() != Some("0");
            (host, allowed)
        });
    let user = text(1)
        .or_else(|| default(b"mysqli.default_user"))
        .unwrap_or_default();
    let password = text(2)
        .or_else(|| default(b"mysqli.default_pw"))
        .unwrap_or_default();
    let port = args::int(args, 4)
        .filter(|port| *port != 0)
        .or_else(|| default(b"mysqli.default_port")?.parse().ok())
        .unwrap_or(3306);
    let mut dsn = Dsn {
        driver: DriverName::Mysql,
        fields: Default::default(),
        options: state.borrow().options.clone(),
    };
    dsn.fields.insert("host".into(), host.into());
    dsn.fields.insert("port".into(), port.to_string());
    dsn.fields
        .insert("dbname".into(), text(3).unwrap_or_default());
    if host == "localhost"
        && let Some(socket) = text(5)
            .or_else(|| default(b"mysqli.default_socket"))
            .filter(|socket| !socket.is_empty())
    {
        dsn.fields.insert("unix_socket".into(), socket);
    }
    if let Some(charset) = &state.borrow().charset {
        dsn.fields.insert("charset".into(), charset.clone());
    }
    dsn.options.mysql.restore_empty_database = true;
    options::flags(
        &mut dsn.options.mysql,
        args::int(args, 6).unwrap_or_default(),
    )?;
    let policy = if persistent {
        Policy::Persistent
    } else {
        Policy::Unpooled
    };
    let lease = match pool::acquire(&dsn, &user, &password, policy).await {
        Ok(lease) => lease,
        Err(error) => {
            errors::set_connect_error(context, Some(&error));
            state.borrow_mut().error = Some(error.clone());
            return errors::report(context, &error, call, true);
        }
    };
    let connection_info = run(&lease, |driver| {
        Box::pin(async move {
            Ok((
                driver.version.clone(),
                driver.thread_id(),
                driver.connection_status(),
                driver.default_charset().to_owned(),
                driver.backslashes_enabled(),
            ))
        })
    })
    .await
    .map_err(|error| NativeError::error(error.detail))?;
    {
        let mut state = state.borrow_mut();
        (state.version, state.thread_id, state.host_info) =
            (connection_info.0, connection_info.1, connection_info.2);
        state.charset = Some(connection_info.3);
        state.backslashes = connection_info.4;
        state.status = Status::Open(lease);
        state.error = None;
    }
    errors::set_connect_error(context, None);
    Ok(Value::Bool(true))
}
