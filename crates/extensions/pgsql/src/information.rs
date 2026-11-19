//! `pg_dbname($db)`, `pg_version($db)`, and related connection metadata.

use crate::connection;
use phera_extension_api::{ArrayKey, Context, LocalFuture, NativeResult, PhpArray, Value, args};

pub const DBNAME: usize = 0;
pub const HOST: usize = 1;
pub const PORT: usize = 2;
pub const OPTIONS: usize = 3;
pub const VERSION: usize = 4;
pub const ENCODING: usize = 5;
pub const LAST_ERROR: usize = 6;
pub const PID: usize = 7;
pub const STATUS: usize = 8;
pub const TRANSACTION: usize = 9;
pub const NOTICE: usize = 10;
pub const NOTIFY: usize = 11;
pub const PING: usize = 12;
pub const PARAMETER: usize = 13;
pub const SET_ENCODING: usize = 14;
pub const POLL: usize = 15;
pub const TTY: usize = 16;

pub fn jit(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let state = connection::argument(context, args::value(&arguments, 0), "pg_jit")?;
        let lease = state.borrow().lease()?;
        let values = connection::run(&lease, |driver| {
            Box::pin(async move {
                Ok([
                    driver.parameter_status("jit_provider").await.ok().flatten(),
                    driver.parameter_status("jit").await.ok().flatten(),
                ])
            })
        })
        .await
        .map_err(|cause| phera_extension_api::NativeError::error(cause.detail))?;
        let mut result = PhpArray::new();
        for (name, value) in ["jit_provider", "jit"].into_iter().zip(values) {
            result.insert(ArrayKey::str(name), value.map_or(Value::Null, Value::from));
        }
        Ok(Value::from(result))
    })
}

const NAMES: &[&str] = &[
    "pg_dbname",
    "pg_host",
    "pg_port",
    "pg_options",
    "pg_version",
    "pg_client_encoding",
    "pg_last_error",
    "pg_get_pid",
    "pg_connection_status",
    "pg_transaction_status",
    "pg_last_notice",
    "pg_get_notify",
    "pg_ping",
    "pg_parameter_status",
    "pg_set_client_encoding",
    "pg_connect_poll",
    "pg_tty",
];

pub fn sync<const OP: usize>(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    sync_named::<OP>(c, a, NAMES.get(OP).copied().unwrap_or_default())
}

pub(crate) fn sync_named<const OP: usize>(
    c: &mut dyn Context,
    a: &[Value],
    name: &str,
) -> NativeResult {
    let state = connection::argument(c, args::value(a, 0), name)?;
    let state = state.borrow();
    let lease = state.lease()?;
    Ok(match OP {
        DBNAME => Value::from(state.dsn.get("dbname", "").to_owned()),
        HOST => Value::from(state.dsn.get("host", "localhost").to_owned()),
        PORT => Value::from(state.dsn.get("port", "5432").to_owned()),
        OPTIONS => Value::from(state.dsn.get("options", "").to_owned()),
        TTY => Value::from(state.dsn.get("tty", "").to_owned()),
        ENCODING => Value::from(state.encoding.clone()),
        LAST_ERROR => return connection::last_error(&state),
        PID => Value::Int(state.pid.into()),
        POLL => Value::Int(3),
        STATUS => Value::Int(
            lease
                .try_inspect(|driver| match driver {
                    phera_db_drivers::drivers::Driver::Postgres(pg) => i64::from(pg.is_broken()),
                    _ => 1,
                })
                .unwrap_or(0),
        ),
        TRANSACTION => Value::Int(if state.busy.is_some() {
            1
        } else {
            lease
                .try_inspect(|driver| match driver {
                    phera_db_drivers::drivers::Driver::Postgres(pg) => pg.transaction_status(),
                    _ => 4,
                })
                .unwrap_or(1)
        }),
        _ => Value::Bool(false),
    })
}

pub fn asynchronous<const OP: usize>(
    c: &mut dyn Context,
    a: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    asynchronous_named::<OP>(c, a, NAMES.get(OP).copied().unwrap_or_default())
}

pub(crate) fn asynchronous_named<'a, const OP: usize>(
    c: &'a mut dyn Context,
    a: Vec<Value>,
    name: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let explicit = args::object(&a, 0).is_some();
        let arg = if explicit {
            args::value(&a, 0)
        } else {
            &Value::Null
        };
        let state = connection::argument(c, arg, name)?;
        let lease = state.borrow().lease()?;
        match OP {
            VERSION => {
                let server = state.borrow().server_version.clone();
                let settings = connection::run(&lease, |pg| {
                    Box::pin(async move {
                        let mut values = Vec::new();
                        for name in [
                            "server_encoding",
                            "client_encoding",
                            "is_superuser",
                            "session_authorization",
                            "DateStyle",
                            "IntervalStyle",
                            "TimeZone",
                            "integer_datetimes",
                            "standard_conforming_strings",
                            "application_name",
                        ] {
                            if let Some(value) = pg.parameter_status(name).await? {
                                values.push((name, value));
                            }
                        }
                        Ok(values)
                    })
                })
                .await
                .unwrap_or_default();
                let mut fields = PhpArray::new();
                fields.insert(
                    ArrayKey::str("client"),
                    Value::from(crate::constants::LIBPQ_VERSION),
                );
                fields.insert(ArrayKey::str("protocol"), Value::Int(3));
                fields.insert(ArrayKey::str("server"), Value::from(server));
                for (name, value) in settings {
                    fields.insert(ArrayKey::str(name), Value::from(value));
                }
                Ok(Value::from(fields))
            }
            PING => Ok(Value::Bool(
                connection::run(&lease, |driver| Box::pin(driver.health()))
                    .await
                    .is_ok(),
            )),
            PARAMETER => {
                let key = args::string(&a, usize::from(explicit))
                    .map_or_else(String::new, |value| value.to_string_lossy());
                let value = connection::run(&lease, move |driver| {
                    Box::pin(async move { driver.parameter_status(&key).await })
                })
                .await
                .ok()
                .flatten();
                Ok(value.map_or(Value::Bool(false), Value::from))
            }
            SET_ENCODING => {
                let key = args::string(&a, usize::from(explicit))
                    .map_or_else(String::new, |value| value.to_string_lossy());
                if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                    return Ok(Value::Int(-1));
                }
                let status = connection::run(&lease, move |driver| {
                    Box::pin(async move {
                        let results = driver
                            .native_query(&format!("SET client_encoding TO '{key}'"))
                            .await?;
                        if let Some(error) = results.into_iter().find_map(|result| result.error) {
                            return Err(error);
                        }
                        driver.parameter_status("client_encoding").await
                    })
                })
                .await;
                match status {
                    Ok(Some(encoding)) => {
                        state.borrow_mut().encoding = encoding;
                        Ok(Value::Int(0))
                    }
                    _ => Ok(Value::Int(-1)),
                }
            }
            NOTICE => {
                let mode = args::int(&a, 1).unwrap_or(1);
                let notices = connection::run(&lease, move |pg| {
                    Box::pin(async move { Ok(pg.notices(mode == 3)) })
                })
                .await
                .unwrap_or_default();
                match mode {
                    1 => Ok(Value::from(notices.last().cloned().unwrap_or_default())),
                    2 => {
                        let mut list = PhpArray::new();
                        for notice in notices {
                            list.push(Value::from(notice))?;
                        }
                        Ok(Value::from(list))
                    }
                    3 => Ok(Value::Bool(true)),
                    _ => Err(phera_extension_api::NativeError::value_error(concat!(
                        "pg_last_notice(): Argument #2 ($mode) must be one of ",
                        "PGSQL_NOTICE_LAST, PGSQL_NOTICE_ALL, or PGSQL_NOTICE_CLEAR",
                    ))),
                }
            }
            NOTIFY => {
                let mode = args::int(&a, 1).unwrap_or(1);
                let notification =
                    connection::run(&lease, |pg| Box::pin(async move { Ok(pg.notification()) }))
                        .await
                        .ok()
                        .flatten();
                let Some(notification) = notification else {
                    return Ok(Value::Bool(false));
                };
                let mut result = PhpArray::new();
                for (key, value) in [
                    ("message", Value::from(notification.channel().to_owned())),
                    ("pid", Value::Int(notification.process_id().into())),
                    ("payload", Value::from(notification.payload().to_owned())),
                ] {
                    if mode & 2 != 0 {
                        result.push(value.clone())?;
                    }
                    if mode & 1 != 0 {
                        result.insert(ArrayKey::str(key), value);
                    }
                }
                Ok(Value::from(result))
            }
            _ => Ok(Value::Bool(false)),
        }
    })
}
