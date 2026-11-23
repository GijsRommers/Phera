//! `$r->xRead([$stream => "0-0"])` returns streams indexed by key and entry id.
use super::*;
use crate::connection::number;

pub async fn build(
    name: &str,
    c: &mut dyn Context,
    a: &[Value],
    b: &mut Builder<'_>,
) -> Result<bool, NativeError> {
    match name {
        "xadd" => {
            b.key(&a[0]);
            if a.get(5) == Some(&Value::Bool(true)) {
                b.token("NOMKSTREAM");
            }
            if number(a.get(3), 0.0) > 0.0 {
                b.token("MAXLEN");
                if a.get(4) == Some(&Value::Bool(true)) {
                    b.token("~");
                }
                b.arg(&a[3]);
            }
            b.arg(&a[1]);
            if let Some(values) = array(a.get(2)) {
                for (key, value) in values.iter() {
                    match key {
                        phera_extension_api::ArrayKey::Int(i) => b.token(i),
                        phera_extension_api::ArrayKey::Interned(s) => b.token(s.as_bytes()),
                        phera_extension_api::ArrayKey::String(s) => b.token(s.as_bytes()),
                    }
                    b.value(c, value).await?;
                }
            }
        }
        "xread" | "xreadgroup" => {
            let offset = if name == "xreadgroup" {
                b.token("GROUP");
                b.arg(&a[0]);
                b.arg(&a[1]);
                2
            } else {
                0
            };
            if number(a.get(offset + 1), -1.0) > 0.0 {
                b.token("COUNT");
                b.arg(&a[offset + 1]);
            }
            if number(a.get(offset + 2), -1.0) >= 0.0 {
                b.token("BLOCK");
                b.arg(&a[offset + 2]);
            }
            b.token("STREAMS");
            if let Some(streams) = array(a.get(offset)) {
                for (key, _) in streams.iter() {
                    let key = match key {
                        phera_extension_api::ArrayKey::Int(i) => Value::Int(i),
                        phera_extension_api::ArrayKey::Interned(s) => Value::InternedStr(s),
                        phera_extension_api::ArrayKey::String(s) => Value::String(s.clone()),
                    };
                    b.key(&key);
                }
                for (_, id) in streams.iter() {
                    b.arg(id);
                }
            }
            b.shape = Shape::Streams;
        }
        "xrange" | "xrevrange" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            b.arg(&a[2]);
            if number(a.get(3), -1.0) >= 0.0 {
                b.token("COUNT");
                b.arg(&a[3]);
            }
            b.shape = Shape::Entries;
        }
        "xack" | "xdel" | "xlen" => {
            b.key(&a[0]);
            b.args(&a[1..]);
        }
        "xgroup" => {
            b.arg(&a[0]);
            if let Some(key) = a.get(1) {
                b.key(key);
            }
            for value in a
                .iter()
                .take(4)
                .skip(2)
                .filter(|v| !matches!(v, Value::Null))
            {
                b.arg(value);
            }
            if a.get(4) == Some(&Value::Bool(true)) {
                b.token("MKSTREAM");
            }
            if number(a.get(5), -2.0) >= -1.0 {
                b.token("ENTRIESREAD");
                b.arg(&a[5]);
            }
        }
        "xtrim" => {
            b.key(&a[0]);
            b.token(if a.get(3) == Some(&Value::Bool(true)) {
                "MINID"
            } else {
                "MAXLEN"
            });
            if a.get(2) == Some(&Value::Bool(true)) {
                b.token("~");
            }
            b.arg(&a[1]);
            if number(a.get(4), -1.0) >= 0.0 {
                b.token("LIMIT");
                b.arg(&a[4]);
            }
        }
        "xpending" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            for value in a.iter().skip(2).filter(|v| !matches!(v, Value::Null)) {
                b.arg(value);
            }
        }
        "xclaim" => {
            b.key(&a[0]);
            b.args(&a[1..5]);
            flags(b, a.get(5));
            b.shape = Shape::Entries;
        }
        "xautoclaim" => {
            b.key(&a[0]);
            b.args(&a[1..5]);
            if number(a.get(5), -1.0) >= 0.0 {
                b.token("COUNT");
                b.arg(&a[5]);
            }
            if a.get(6) == Some(&Value::Bool(true)) {
                b.token("JUSTID");
            }
            b.shape = Shape::AutoClaim {
                just_id: a.get(6) == Some(&Value::Bool(true)),
            };
        }
        "xinfo" => {
            b.arg(&a[0]);
            if let Some(key) = a.get(1) {
                b.key(key);
            }
            if let Some(arg) = a.get(2).filter(|v| !matches!(v, Value::Null)) {
                b.arg(arg);
            }
            if number(a.get(3), -1.0) >= 0.0 {
                b.token("COUNT");
                b.arg(&a[3]);
            }
            b.shape = Shape::StreamInfo;
        }
        _ => return Ok(false),
    }
    Ok(true)
}
