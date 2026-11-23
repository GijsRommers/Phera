//! `$r->eval($script, [$key, $argument], 1)` prefixes keys but not Lua arguments.
use super::*;
use crate::connection::number;

pub fn build(name: &str, a: &[Value], b: &mut Builder<'_>) -> Result<bool, NativeError> {
    match name {
        "rawcommand" => {
            b.shape = Shape::Literal;
            b.command = redis::cmd(&string(&a[0]));
            b.args(&a[1..]);
        }
        "eval" | "evalsha" | "eval_ro" | "evalsha_ro" => {
            b.shape = Shape::Literal;
            b.arg(&a[0]);
            let count = number(a.get(2), 0.0) as usize;
            b.token(count);
            if let Some(values) = array(a.get(1)) {
                for (i, (_, value)) in values.iter().enumerate() {
                    if i < count {
                        b.key(value);
                    } else {
                        b.arg(value);
                    }
                }
            }
        }
        "fcall" | "fcall_ro" => {
            b.shape = Shape::Literal;
            b.arg(&a[0]);
            b.token(array(a.get(1)).map_or(0, |a| a.len()));
            if let Some(keys) = a.get(1) {
                b.keys(std::slice::from_ref(keys));
            }
            if let Some(args) = a.get(2) {
                b.args(std::slice::from_ref(args));
            }
        }
        "script" | "function" | "client" | "command" | "pubsub" | "slowlog" | "acl" => {
            b.args(a);
            if name == "function"
                && matches!(string(&a[0]).to_uppercase().as_str(), "LIST" | "STATS")
            {
                b.shape = Shape::Zipped;
            }
            if name == "client"
                && a.first()
                    .is_some_and(|v| string(v).eq_ignore_ascii_case("GETNAME"))
            {
                b.shape = Shape::Raw;
            }
            // `pubsub('numsub', $channels)` answers channel => subscribers.
            if name == "pubsub"
                && a.first().is_some_and(|v| {
                    let subcommand = string(v);
                    subcommand.eq_ignore_ascii_case("NUMSUB")
                        || subcommand.eq_ignore_ascii_case("SHARDNUMSUB")
                })
            {
                b.shape = Shape::RawPairs;
            }
        }
        "info" => {
            b.args(a);
            b.shape = Shape::Info;
        }
        "config" => {
            b.arg(&a[0]);
            if string(&a[0]).eq_ignore_ascii_case("SET") {
                if let Some(values) = array(a.get(1)) {
                    for (key, value) in values.iter() {
                        match key {
                            phera_extension_api::ArrayKey::Int(i) => b.token(i),
                            phera_extension_api::ArrayKey::String(s) => b.token(s.as_bytes()),
                        }
                        b.arg(value);
                    }
                } else {
                    b.args(&a[1..]);
                }
            } else {
                b.args(&a[1..]);
                if string(&a[0]).eq_ignore_ascii_case("GET") {
                    b.shape = Shape::RawPairs;
                }
            }
        }
        "object" => {
            b.arg(&a[0]);
            b.key(&a[1]);
        }
        "publish" => {
            b.key(&a[0]);
            b.arg(&a[1]);
        }
        "ping" | "echo" => {
            b.args(a);
        }
        "dbsize" | "time" | "lastsave" | "role" | "unwatch" | "reset" => (),
        "wait" | "waitaof" => b.args(a),
        "debug" => {
            b.token("OBJECT");
            b.key(&a[0]);
        }
        "move" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            b.shape = Shape::Bool;
        }
        "bgsave" | "bgrewriteaof" | "save" => (),
        "swapdb" => b.args(a),
        "replicaof" => {
            if a.first().is_none_or(|v| matches!(v, Value::Null)) {
                b.token("NO");
                b.token("ONE");
            } else {
                b.arg(&a[0]);
                b.token(number(a.get(1), 6379.0) as i64);
            }
        }
        "flushdb" | "flushall" => {
            if let Some(Value::Bool(sync)) = a.first() {
                b.token(if *sync { "SYNC" } else { "ASYNC" });
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
