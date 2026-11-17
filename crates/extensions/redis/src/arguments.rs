//! Some phpredis C parsers intentionally differ from their reflected union signatures.
//! For example, `Redis::bitop()` returns false and deprecated aliases warn before parsing.
use phera_extension_api::{Context, MethodPreparation, NativeError, Param, ParamType, Value};

pub fn prepare(
    name: &str,
    c: &mut dyn Context,
    reflected: &[Param],
    a: &mut [Value],
) -> Result<MethodPreparation, NativeError> {
    let method = name.strip_prefix("Redis::").unwrap_or(name).to_lowercase();
    if matches!(
        method.as_str(),
        "delete"
            | "open"
            | "popen"
            | "slaveof"
            | "sortasc"
            | "sortdesc"
            | "sortascalpha"
            | "sortdescalpha"
    ) {
        c.deprecated(format!("Method {name}() is deprecated"));
    }
    let mut params = reflected.to_vec();
    match method.as_str() {
        "bitop" => {
            if a.len() < 3 || a.iter().any(|v| matches!(v, Value::Object(_))) {
                return Ok(MethodPreparation::Return(Value::Bool(false)));
            }
        }
        "geoadd" | "spop" | "zpopmin" | "zpopmax" if a.is_empty() => {
            return Err(NativeError::argument_count_error(format!(
                "Wrong parameter count for {name}()"
            )));
        }
        "hset" | "lpush" | "rpush" | "zadd" => {
            let required = if method == "zadd" { 3 } else { 2 };
            if a.len() < required {
                return Err(NativeError::argument_count_error(format!(
                    "{name}() expects at least {required} arguments, {} given",
                    a.len()
                )));
            }
        }
        "zrangebylex" | "zrevrangebylex" if !matches!(a.len(), 3 | 5) => {
            c.warning(format!("{name}(): Must pass either 3 or 5 arguments"));
            return Ok(MethodPreparation::Return(Value::Bool(false)));
        }
        "rawcommand" if a.first().is_some_and(|v| !matches!(v, Value::String(_))) => {
            c.warning(format!(
                "{name}(): When sending a 'raw' command, the first argument must be a string!"
            ));
            return Ok(MethodPreparation::Return(Value::Bool(false)));
        }
        "script" if a.first().is_some_and(|v| !matches!(v, Value::String(_))) => {
            return Ok(MethodPreparation::Return(Value::Bool(false)));
        }
        "pfcount"
            if a.first()
                .is_some_and(|v| !matches!(v, Value::String(_) | Value::Array(_))) =>
        {
            c.warning(format!(
                "{name}(): Argument must be either an array or a string"
            ));
            return Ok(MethodPreparation::Return(Value::Bool(false)));
        }
        "xclaim" => params[5].optional = true,
        "sdiffstore" | "sunionstore" => params[1].optional = true,
        "sort" | "sort_ro" => params[1].nullable = false,
        "command" | "slaveof" | "replicaof"
            if a.first().is_some_and(|v| matches!(v, Value::Object(_))) =>
        {
            // These implementations use a non-nullable parser despite nullable stubs.
            params[0].nullable = false;
        }
        _ => (),
    }
    if matches!(
        method.as_str(),
        "blpop"
            | "brpop"
            | "bzpopmin"
            | "bzpopmax"
            | "del"
            | "delete"
            | "exists"
            | "sdiff"
            | "sdiffstore"
            | "sinter"
            | "sinterstore"
            | "sunion"
            | "sunionstore"
            | "touch"
            | "unlink"
            | "watch"
            | "geoadd"
    ) {
        for param in &mut params {
            param.ty = ParamType::Mixed;
        }
        phera_extension_api::prepare_call(name, &params, a, c)?;
        if a.first().is_some_and(|v| matches!(v, Value::Object(_))) {
            let object = &a[0];
            phera_extension_api::value::to_php_string(object, c.diagnostics())?;
        }
        return Ok(MethodPreparation::Prepared);
    }
    if params != reflected {
        phera_extension_api::prepare_call(name, &params, a, c)?;
        Ok(MethodPreparation::Prepared)
    } else {
        Ok(MethodPreparation::Default)
    }
}
