//! PHP-visible connection state stays in Object::native on its worker thread.
use crate::options::Options;
use crate::pool::{Endpoint, Lease};
use crate::transaction::Batch;
use phera_extension_api::{NativeError, NativeResult, Object, Value};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct Session {
    pub lease: RefCell<Option<Lease>>,
    pub enabled: Cell<bool>,
    pub endpoint: RefCell<Option<Endpoint>>,
    pub database: Cell<i64>,
    pub auth: RefCell<Value>,
    pub options: RefCell<Options>,
    pub error: RefCell<Option<String>>,
    pub batch: RefCell<Option<Batch>>,
    pub subscriptions: RefCell<Option<crate::pubsub::Subscription>>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            lease: RefCell::new(None),
            enabled: Cell::new(false),
            endpoint: RefCell::new(None),
            database: Cell::new(0),
            auth: RefCell::new(Value::Null),
            options: RefCell::new(Options::default()),
            error: RefCell::new(None),
            batch: RefCell::new(None),
            subscriptions: RefCell::new(None),
        }
    }
}

pub fn session(object: &Rc<Object>) -> Rc<Session> {
    if let Some(session) = object
        .native
        .borrow()
        .as_ref()
        .and_then(|n| n.downcast_ref::<Rc<Session>>())
        .cloned()
    {
        return session;
    }
    let session = Rc::new(Session::default());
    *object.native.borrow_mut() = Some(Box::new(session.clone()));
    session
}

/// The fork hook of `Redis`: a fork's copy has the boot's endpoint,
/// credentials, database and options but no socket; its first command
/// borrows one from the pool and sends `AUTH` and `SELECT` again (see
/// [`Session::ensure`]). The pool resets a socket before lending it again.
///
/// ```php
/// $redis = new Redis();
/// $redis->connect('127.0.0.1');
/// $redis->select(2);
/// Phera\snapshot();
/// $redis->get('key');   // each request: its own socket, database 2
/// ```
pub fn fork_session(
    _object: &Object,
    state: &dyn std::any::Any,
    fork: &mut phera_extension_api::value::Fork<'_>,
) -> Result<Box<dyn std::any::Any>, String> {
    const ADVICE: &str = "; finish it before Phera\\snapshot(), or connect after the call";
    let session = state
        .downcast_ref::<Rc<Session>>()
        .ok_or("its built-in state is not initialized")?;
    if session.batch.borrow().is_some() {
        return Err(format!("a MULTI or pipeline is open on it{ADVICE}"));
    }
    if session.subscriptions.borrow().is_some() {
        return Err(format!("it is subscribed to channels{ADVICE}"));
    }
    let copy = Session {
        lease: RefCell::new(None),
        enabled: Cell::new(session.enabled.get()),
        endpoint: RefCell::new(session.endpoint.borrow().clone()),
        database: Cell::new(session.database.get()),
        auth: RefCell::new(fork.value(&session.auth.borrow())),
        options: RefCell::new(session.options.borrow().clone()),
        error: RefCell::new(session.error.borrow().clone()),
        batch: RefCell::new(None),
        subscriptions: RefCell::new(None),
    };
    Ok(Box::new(Rc::new(copy)) as Box<dyn std::any::Any>)
}

pub fn exception(message: impl Into<String>) -> NativeError {
    NativeError::throw("RedisException", message).with_code(0)
}

impl Session {
    pub fn connected(&self) -> Result<Lease, NativeError> {
        self.lease
            .borrow()
            .clone()
            .ok_or_else(|| exception("Redis server went away"))
    }
    pub async fn ensure(&self) -> Result<Lease, NativeError> {
        if self.lease.borrow().is_none() && self.enabled.get() {
            // `enabled` is only set together with an endpoint.
            let endpoint = self
                .endpoint
                .borrow()
                .clone()
                .ok_or_else(|| exception("Redis server went away"))?;
            let lease = crate::pool::acquire(endpoint)
                .await
                .map_err(|e| exception(system_message(&e)))?;
            let auth = self.auth.borrow().clone();
            let read_timeout = self.options.borrow().read_timeout;
            if !matches!(auth, Value::Null) {
                let mut command = redis::cmd("AUTH");
                match auth {
                    Value::Array(values) => {
                        for (_, v) in values.iter() {
                            command.arg(string(v));
                        }
                    }
                    _ => {
                        command.arg(string(&auth));
                    }
                }
                let reply = lease
                    .query(command, read_timeout)
                    .await
                    .map_err(|e| exception(system_message(&e)))?;
                if let redis::Value::ServerError(error) = reply {
                    return Err(exception(format!(
                        "{} {}",
                        error.code(),
                        error.details().unwrap_or_default()
                    )));
                }
            }
            if self.database.get() != 0 {
                let mut command = redis::cmd("SELECT");
                command.arg(self.database.get());
                let reply = lease
                    .query(command, read_timeout)
                    .await
                    .map_err(|e| exception(system_message(&e)))?;
                if let redis::Value::ServerError(error) = reply {
                    return Err(exception(format!(
                        "{} {}",
                        error.code(),
                        error.details().unwrap_or_default()
                    )));
                }
            }
            *self.lease.borrow_mut() = Some(lease);
        }
        self.connected()
    }

    pub async fn query(&self, command: redis::Cmd) -> Result<redis::Value, NativeError> {
        let lease = self.ensure().await?;
        let timeout = self.options.borrow().read_timeout;
        let result = lease.query(command, timeout).await.map_err(|error| {
            let message = match (error.code(), error.detail()) {
                (Some(code), Some(detail)) => format!("{code} {detail}"),
                _ => error.to_string(),
            };
            *self.error.borrow_mut() = Some(message.clone());
            exception(message)
        })?;
        if let redis::Value::ServerError(error) = &result {
            let message = format!("{} {}", error.code(), error.details().unwrap_or_default());
            *self.error.borrow_mut() = Some(message.clone());
            if throws(&message) {
                return Err(exception(message));
            }
        }
        Ok(result)
    }
}

pub async fn connect(name: &str, session: &Session, arguments: &[Value]) -> NativeResult {
    let host = string(&arguments[0]);
    let port = number(arguments.get(1), 6379.0) as i64;
    let timeout = number(arguments.get(2), 0.0);
    let read_timeout = number(arguments.get(5), 0.0);
    if timeout > f64::from(i32::MAX) || !timeout.is_finite() {
        return Err(NativeError::value_error("Invalid connect timeout"));
    }
    if read_timeout > f64::from(i32::MAX) || !read_timeout.is_finite() {
        return Err(NativeError::value_error("Invalid read timeout"));
    }
    let retry = number(arguments.get(4), 0.0);
    if retry < 0.0 || retry > f64::from(i32::MAX) {
        return Err(NativeError::value_error("Invalid retry interval"));
    }
    let persistent = matches!(name, "pconnect" | "popen");
    let persistent_id = if persistent {
        arguments
            .get(3)
            .filter(|v| !matches!(v, Value::Null))
            .map(string)
    } else {
        None
    };
    let endpoint = Endpoint {
        host,
        port: u16::try_from(port).unwrap_or(6379),
        timeout_micros: (timeout * 1_000_000.0) as u64,
        persistent_id,
    };
    session.lease.borrow_mut().take();
    let lease = crate::pool::acquire(endpoint.clone())
        .await
        .map_err(|e| exception(system_message(&e)))?;
    *session.endpoint.borrow_mut() = Some(endpoint);
    *session.lease.borrow_mut() = Some(lease);
    session.enabled.set(true);
    session.database.set(0);
    *session.options.borrow_mut() = Options {
        read_timeout,
        ..Default::default()
    };
    *session.auth.borrow_mut() = Value::Null;
    if let Some(value) = crate::commands::option(arguments.get(6), "auth") {
        auth(session, value).await?;
    }
    Ok(Value::Bool(true))
}

pub async fn auth(session: &Session, value: &Value) -> NativeResult {
    let mut command = redis::cmd("AUTH");
    match value {
        Value::Array(values) => {
            for (_, value) in values.iter() {
                command.arg(string(value));
            }
        }
        _ => {
            command.arg(string(value));
        }
    }
    // phpredis remembers the attempted credentials even when AUTH is rejected.
    let result = session.query(command).await;
    *session.auth.borrow_mut() = value.clone();
    let reply = result?;
    if !matches!(reply, redis::Value::Okay) {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(true))
}

pub async fn select(session: &Session, value: &Value) -> NativeResult {
    let db = number(Some(value), 0.0) as i64;
    let mut command = redis::cmd("SELECT");
    command.arg(db);
    let reply = session.query(command).await?;
    if !matches!(reply, redis::Value::Okay) {
        return Ok(Value::Bool(false));
    }
    session.database.set(db);
    Ok(Value::Bool(true))
}

pub fn string(value: &Value) -> String {
    match value {
        Value::String(s) => s.to_string_lossy(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(true) => "1".into(),
        _ => String::new(),
    }
}

pub fn number(value: Option<&Value>, default: f64) -> f64 {
    match value {
        Some(Value::Int(i)) => *i as f64,
        Some(Value::Float(f)) => *f,
        Some(Value::String(s)) => s.to_string_lossy().parse().unwrap_or(default),
        _ => default,
    }
}

/// phpredis library.c redis_error_throw preserves its explicit non-throwing error prefixes.
pub fn throws(message: &str) -> bool {
    if message.starts_with("ERR") && !message.starts_with("ERR AUTH") {
        return false;
    }
    ![
        "NOSCRIPT",
        "NOQUORUM",
        "NOGOODSLAVE",
        "WRONGTYPE",
        "BUSYGROUP",
        "NOGROUP",
    ]
    .iter()
    .any(|prefix| message.starts_with(prefix))
}

/// An error's text as PHP shows it: Rust adds " (os error 61)" to a message
/// from the operating system, phpredis prints "Connection refused" alone.
fn system_message(error: &impl std::fmt::Display) -> String {
    let message = error.to_string();
    match message.rfind(" (os error ") {
        Some(start) if message.ends_with(')') => message[..start].to_owned(),
        _ => message,
    }
}
