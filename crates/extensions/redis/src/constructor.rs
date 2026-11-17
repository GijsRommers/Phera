//! phpredis 6 constructors configure a lazy socket; getters or commands open it later.
use crate::connection::{Session, number, string};
use crate::pool::Endpoint;
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, Value};

pub fn construct(c: &mut dyn Context, s: &Session, args: &[Value]) -> NativeResult {
    let mut endpoint = Endpoint {
        host: "127.0.0.1".into(),
        port: 6379,
        timeout_micros: 0,
        persistent_id: None,
    };
    if let Some(options) = crate::commands::array(args.first()) {
        for (key, value) in options.iter() {
            let ArrayKey::String(key) = key else {
                continue;
            };
            let name = key.to_string_lossy();
            match name.to_lowercase().as_str() {
                "host" => {
                    if !matches!(value, Value::String(_)) {
                        return Err(invalid("host"));
                    }
                    endpoint.host = string(value);
                }
                "port" => {
                    if !matches!(value, Value::Int(_)) {
                        return Err(invalid("port"));
                    }
                    endpoint.port = number(Some(value), 6379.0) as u16;
                }
                "connecttimeout" | "readtimeout" => {
                    if !matches!(value, Value::Int(_) | Value::Float(_)) {
                        return Err(invalid(if name.eq_ignore_ascii_case("connectTimeout") {
                            "connect timeout"
                        } else {
                            "read timeout"
                        }));
                    }
                    let timeout = number(Some(value), 0.0);
                    if name.eq_ignore_ascii_case("connectTimeout") {
                        endpoint.timeout_micros = (timeout * 1_000_000.0) as u64;
                    } else {
                        s.options.borrow_mut().read_timeout = timeout;
                    }
                }
                "persistent" => {
                    if let Value::String(id) = value {
                        endpoint.persistent_id = Some(id.to_string_lossy());
                    }
                }
                "database" => {
                    let Value::Int(db) = value else {
                        return Err(invalid("database number"));
                    };
                    if *db < 0 || *db > i64::from(i32::MAX) {
                        return Err(invalid("database number"));
                    }
                    s.database.set(*db);
                }
                "auth" => {
                    if !matches!(value, Value::Array(_) | Value::String(_)) {
                        return Err(invalid("auth credentials"));
                    }
                    *s.auth.borrow_mut() = value.clone();
                }
                "maxretries" => {
                    if !matches!(value,Value::Int(n) if *n >= 0) {
                        return Err(NativeError::value_error(
                            "Max retries must be a non-negative integer",
                        ));
                    }
                }
                "retryinterval" => {
                    if !matches!(value, Value::Int(_) | Value::Float(_)) {
                        return Err(invalid("retry interval"));
                    }
                }
                "ssl" | "backoff" => {
                    if !matches!(value, Value::Array(_)) {
                        return Err(invalid("options"));
                    }
                }
                _ => c.warning(format!(
                    "Redis::__construct(): Skip unknown option '{name}'"
                )),
            }
        }
    }
    *s.endpoint.borrow_mut() = Some(endpoint);
    s.enabled.set(true);
    Ok(Value::Null)
}

fn invalid(field: &str) -> NativeError {
    NativeError::value_error(format!("Invalid {field}"))
}
