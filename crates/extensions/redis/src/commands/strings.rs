//! `$r->set("key", $value, ["nx", "px" => 5000])` keeps options separate from data.
use super::*;
use crate::connection::number;

pub async fn build(
    name: &str,
    context: &mut dyn Context,
    a: &[Value],
    b: &mut Builder<'_>,
) -> Result<bool, NativeError> {
    match name {
        "get" | "getdel" | "getex" | "getwithmeta" => {
            if name == "getwithmeta" {
                b.command = redis::cmd("GET");
            }
            b.key(&a[0]);
            if name == "getex" {
                flags(b, a.get(1));
            }
            b.shape = if name == "getwithmeta" {
                Shape::Metadata
            } else {
                Shape::Value
            };
        }
        "set" => {
            b.key(&a[0]);
            b.value(context, &a[1]).await?;
            if let Some(Value::Array(options)) = a.get(2) {
                for (key, value) in options.iter() {
                    match key {
                        phera_extension_api::ArrayKey::Int(_) => {
                            let flag = string(value).to_uppercase();
                            if flag == "GET" {
                                b.shape = Shape::Value;
                            }
                            if matches!(flag.as_str(), "NX" | "XX" | "GET" | "KEEPTTL") {
                                b.token(flag);
                            }
                        }
                        phera_extension_api::ArrayKey::Interned(key) => {
                            let flag = key.to_string_lossy().to_uppercase();
                            if matches!(flag.as_str(), "EX" | "PX" | "EXAT" | "PXAT") {
                                b.token(flag);
                                b.arg(value);
                            }
                        }
                        phera_extension_api::ArrayKey::String(key) => {
                            let flag = key.to_string_lossy().to_uppercase();
                            if matches!(flag.as_str(), "EX" | "PX" | "EXAT" | "PXAT") {
                                b.token(flag);
                                b.arg(value);
                            }
                        }
                    }
                }
            } else if let Some(timeout) = a.get(2) {
                let timeout = number(Some(timeout), 0.0) as i64;
                if timeout > 0 {
                    b.token("EX");
                    b.token(timeout);
                }
            }
        }
        "setex" | "psetex" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            b.value(context, &a[2]).await?;
        }
        "setnx" | "getset" | "append" | "delifeq" => {
            b.key(&a[0]);
            b.value(context, &a[1]).await?;
            b.shape = match name {
                "getset" => Shape::Value,
                "setnx" | "delifeq" => Shape::Bool,
                _ => Shape::Raw,
            };
        }
        "mget" => {
            b.keys(a);
            b.shape = Shape::Values;
        }
        "mset" | "msetnx" => {
            if let Some(values) = array(a.first()) {
                for (key, value) in values.iter() {
                    let key = match key {
                        phera_extension_api::ArrayKey::Int(i) => Value::Int(i),
                        phera_extension_api::ArrayKey::Interned(s) => Value::InternedStr(s),
                        phera_extension_api::ArrayKey::String(s) => Value::String(s.clone()),
                    };
                    b.key(&key);
                    b.value(context, value).await?;
                }
            }
            if name == "msetnx" {
                b.shape = Shape::Bool;
            }
        }
        "incr" | "decr" => {
            b.command = redis::cmd(if name == "incr" { "INCRBY" } else { "DECRBY" });
            b.key(&a[0]);
            b.token(number(a.get(1), 1.0) as i64);
        }
        "incrby" | "decrby" | "incrbyfloat" | "getrange" | "setrange" | "getbit" | "setbit" => {
            b.key(&a[0]);
            b.args(&a[1..]);
            if name == "incrbyfloat" {
                b.shape = Shape::Float;
            }
        }
        "bitcount" | "bitpos" => {
            b.key(&a[0]);
            let flag = if name == "bitcount" { 3 } else { 4 };
            for value in a.iter().take(flag).skip(1) {
                b.arg(value);
            }
            if a.get(flag).is_some() {
                b.token(if a[flag] == Value::Bool(true) {
                    "BIT"
                } else {
                    "BYTE"
                });
            }
        }
        "bitop" => {
            b.arg(&a[0]);
            b.keys(&a[1..]);
        }
        "strlen" | "ttl" | "pttl" | "expiretime" | "pexpiretime" | "dump" => b.key(&a[0]),
        "type" => {
            b.key(&a[0]);
            b.shape = Shape::Type;
        }
        "persist" => {
            b.key(&a[0]);
            b.shape = Shape::Bool;
        }
        "expire" | "expireat" | "pexpire" | "pexpireat" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            if let Some(mode) = a.get(2).filter(|v| !matches!(v, Value::Null)) {
                b.arg(mode);
            }
            b.shape = Shape::Bool;
        }
        "del" | "delete" | "unlink" | "exists" | "touch" | "watch" => {
            if name == "delete" {
                b.command = redis::cmd("DEL");
            }
            b.keys(a);
        }
        "rename" | "renamenx" | "copy" => {
            b.key(&a[0]);
            b.key(&a[1]);
            if name == "copy" {
                if let Some(db) = option(a.get(2), "db") {
                    b.token("DB");
                    b.arg(db);
                }
                if option(a.get(2), "replace").is_some_and(phera_extension_api::value::to_bool) {
                    b.token("REPLACE");
                }
            }
            if name != "rename" {
                b.shape = Shape::Bool;
            }
        }
        "restore" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            b.arg(&a[2]);
            flags(b, a.get(3));
        }
        "keys" => b.key(&a[0]),
        "randomkey" => (),
        "lcs" => {
            b.key(&a[0]);
            b.key(&a[1]);
            lcs_options(b, a.get(2));
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// `lcs($a, $b, ['idx' => true, 'minmatchlen' => 4])`: LEN, IDX and
/// WITHMATCHLEN are flags, sent when true, and IDX wins over LEN, as
/// Redis refuses both; MINMATCHLEN takes its number.
fn lcs_options(b: &mut Builder<'_>, options: Option<&Value>) {
    let flag = |name| option(options, name).is_some_and(phera_extension_api::value::to_bool);
    if flag("idx") {
        b.token("IDX");
    } else if flag("len") {
        b.token("LEN");
    }
    if let Some(length) = option(options, "minmatchlen") {
        b.token("MINMATCHLEN");
        b.arg(length);
    }
    if flag("withmatchlen") {
        b.token("WITHMATCHLEN");
    }
}
