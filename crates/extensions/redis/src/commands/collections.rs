//! Hash fields, list elements and set members have different prefix/serialization rules.
use super::*;
use crate::connection::number;

pub async fn build(
    name: &str,
    c: &mut dyn Context,
    a: &[Value],
    b: &mut Builder<'_>,
) -> Result<bool, NativeError> {
    match name {
        "hsetex" => {
            b.key(&a[0]);
            flags(b, a.get(2));
            b.token("FIELDS");
            if let Some(values) = array(a.get(1)) {
                b.token(values.len());
                for (key, value) in values.iter() {
                    match key {
                        phera_extension_api::ArrayKey::Int(i) => b.token(*i),
                        phera_extension_api::ArrayKey::String(s) => b.token(s.as_bytes()),
                    }
                    b.value(c, value).await?;
                }
            }
        }
        "expiremember" | "expirememberat" => {
            b.key(&a[0]);
            b.args(&a[1..]);
        }
        "hget" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            b.shape = Shape::Value;
        }
        "hmget" => {
            b.key(&a[0]);
            b.args(&a[1..]);
            b.shape = Shape::Fields(
                array(a.get(1))
                    .map(|a| a.iter().map(|(_, v)| string(v).into_bytes()).collect())
                    .unwrap_or_default(),
            );
        }
        "hgetall" => {
            b.key(&a[0]);
            b.shape = Shape::Pairs;
        }
        "hvals" => {
            b.key(&a[0]);
            b.shape = Shape::Values;
        }
        "hset" | "hmset" => {
            b.key(&a[0]);
            if let Some(values) = array(a.get(1)) {
                for (key, value) in values.iter() {
                    match key {
                        phera_extension_api::ArrayKey::Int(i) => b.token(*i),
                        phera_extension_api::ArrayKey::String(s) => b.token(s.as_bytes()),
                    }
                    b.value(c, value).await?;
                }
            } else {
                for pair in a[1..].chunks(2) {
                    b.arg(&pair[0]);
                    if let Some(value) = pair.get(1) {
                        b.value(c, value).await?;
                    }
                }
            }
        }
        "hsetnx" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            b.value(c, &a[2]).await?;
            b.shape = Shape::Bool;
        }
        "hdel" | "hexists" | "hstrlen" | "hincrby" | "hincrbyfloat" | "hkeys" | "hlen" => {
            b.key(&a[0]);
            b.args(&a[1..]);
            b.shape = match name {
                "hexists" => Shape::Bool,
                "hincrbyfloat" => Shape::Float,
                _ => Shape::Raw,
            };
        }
        "hrandfield" => {
            b.key(&a[0]);
            if let Some(count) = option(a.get(1), "count") {
                b.arg(count);
            }
            if option(a.get(1), "withvalues").is_some_and(phera_extension_api::value::to_bool) {
                b.token("WITHVALUES");
                b.shape = Shape::Pairs;
            }
        }
        "hexpire" | "hpexpire" | "hexpireat" | "hpexpireat" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            if let Some(mode) = a.get(3).filter(|v| !matches!(v, Value::Null)) {
                b.arg(mode);
            }
            fields(b, a.get(2));
        }
        "httl" | "hpttl" | "hexpiretime" | "hpexpiretime" | "hpersist" | "hgetdel" => {
            b.key(&a[0]);
            fields(b, a.get(1));
            if name == "hgetdel" {
                b.shape = field_shape(a.get(1));
            }
        }
        "hgetex" => {
            b.key(&a[0]);
            flags(b, a.get(2));
            fields(b, a.get(1));
            b.shape = field_shape(a.get(1));
        }
        "lpush" | "rpush" | "lpushx" | "rpushx" | "sadd" | "srem" => {
            b.key(&a[0]);
            for value in &a[1..] {
                b.value(c, value).await?;
            }
        }
        "saddarray" => {
            b.command = redis::cmd("SADD");
            b.key(&a[0]);
            if let Some(values) = array(a.get(1)) {
                for (_, value) in values.iter() {
                    b.value(c, value).await?;
                }
            }
        }
        "lpop" | "rpop" | "spop" | "srandmember" => {
            b.key(&a[0]);
            b.shape = Shape::Value;
            if let Some(count) = a.get(1)
                && number(Some(count), 0.0) != 0.0
            {
                b.arg(count);
                b.shape = Shape::Values;
            }
        }
        "lindex" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            b.shape = Shape::Value;
        }
        "lrange" | "smembers" => {
            b.key(&a[0]);
            b.args(&a[1..]);
            b.shape = Shape::Values;
        }
        "llen" | "scard" | "ltrim" => {
            b.key(&a[0]);
            b.args(&a[1..]);
        }
        "lset" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            b.value(c, &a[2]).await?;
        }
        "lrem" => {
            b.key(&a[0]);
            b.token(number(a.get(2), 0.0) as i64);
            b.value(c, &a[1]).await?;
        }
        "linsert" => {
            b.key(&a[0]);
            b.arg(&a[1]);
            b.value(c, &a[2]).await?;
            b.value(c, &a[3]).await?;
        }
        "lpos" => {
            b.key(&a[0]);
            b.value(c, &a[1]).await?;
            flags(b, a.get(2));
        }
        "lmove" | "blmove" | "rpoplpush" | "brpoplpush" | "smove" => {
            b.key(&a[0]);
            b.key(&a[1]);
            if name == "smove" {
                b.value(c, &a[2]).await?;
                b.shape = Shape::Bool;
            } else {
                b.args(&a[2..]);
                b.shape = Shape::Value;
            }
        }
        "blpop" | "brpop" | "bzpopmin" | "bzpopmax" => {
            if matches!(a.first(), Some(Value::Array(_))) {
                b.keys(&a[..1]);
                b.arg(&a[1]);
            } else {
                b.keys(&a[..a.len() - 1]);
                b.arg(&a[a.len() - 1]);
            }
            b.shape = Shape::Pop {
                sorted: name.starts_with("bz"),
            };
        }
        "lmpop" | "blmpop" | "zmpop" | "bzmpop" => {
            let offset = usize::from(name.starts_with('b'));
            if offset > 0 {
                b.arg(&a[0]);
            }
            b.token(array(a.get(offset)).map_or(0, |a| a.len()));
            b.keys(&a[offset..offset + 1]);
            b.arg(&a[offset + 1]);
            b.token("COUNT");
            b.token(number(a.get(offset + 2), 1.0) as i64);
            b.shape = Shape::MultiPop {
                sorted: name.contains('z'),
            };
        }
        "sismember" => {
            b.key(&a[0]);
            b.value(c, &a[1]).await?;
            b.shape = Shape::Bool;
        }
        "smismember" => {
            b.key(&a[0]);
            b.args(&a[1..]);
        }
        "sdiff" | "sinter" | "sunion" => {
            b.keys(a);
            b.shape = Shape::Values;
        }
        "sdiffstore" | "sinterstore" | "sunionstore" => b.keys(a),
        "sintercard" => {
            b.token(array(a.first()).map_or(0, |a| a.len()));
            b.keys(&a[..1]);
            if number(a.get(1), -1.0) >= 0.0 {
                b.token("LIMIT");
                b.arg(&a[1]);
            }
        }
        "pfadd" => {
            b.key(&a[0]);
            b.args(&a[1..]);
        }
        "pfcount" | "pfmerge" => b.keys(a),
        _ => return Ok(false),
    }
    Ok(true)
}

fn fields(b: &mut Builder<'_>, value: Option<&Value>) {
    b.token("FIELDS");
    b.token(array(value).map_or(0, |v| v.len()));
    if let Some(Value::Array(values)) = value {
        for (_, value) in values.iter() {
            b.arg(value);
        }
    }
}

fn field_shape(value: Option<&Value>) -> Shape {
    Shape::Fields(
        array(value)
            .map(|a| a.iter().map(|(_, v)| string(v).into_bytes()).collect())
            .unwrap_or_default(),
    )
}
