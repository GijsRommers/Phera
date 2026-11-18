//! SCAN writes its cursor through the PHP reference; retry only repeats empty non-final pages.
use crate::connection::{Session, exception, number};
use crate::reply::{Shape, items};
use phera_extension_api::{Context, NativeResult, Value};

pub async fn scan(name: &str, c: &mut dyn Context, s: &Session, a: &[Value]) -> NativeResult {
    if s.batch.borrow().is_some() {
        return Err(exception(
            "Can't call SCAN commands in multi or pipeline mode!",
        ));
    }
    let offset = usize::from(name != "scan");
    let Value::Reference(reference) = &a[offset] else {
        return Ok(Value::Bool(false));
    };
    let initial = reference.borrow().clone();
    if initial == Value::Int(0) {
        return Ok(Value::Bool(false));
    }
    let mut cursor = number(Some(&initial), 0.0) as u64;
    let options = s.options.borrow().clone();
    loop {
        let mut b = crate::commands::Builder::new(name, &options);
        if offset > 0 {
            b.key(&a[0]);
        }
        b.token(cursor);
        if let Some(pattern) = a.get(offset + 1).filter(|v| !matches!(v, Value::Null)) {
            b.token("MATCH");
            if offset == 0 && options.scan & 2 != 0 {
                b.key(pattern);
            } else {
                b.arg(pattern);
            }
        }
        if number(a.get(offset + 2), 0.0) > 0.0 {
            b.token("COUNT");
            b.arg(&a[offset + 2]);
        }
        if offset == 0
            && let Some(kind) = a.get(3).filter(|v| !matches!(v, Value::Null))
        {
            b.token("TYPE");
            b.arg(kind);
        }
        let mut reply = items(s.query(b.command).await?).into_iter();
        cursor = reply
            .next()
            .map(crate::reply::bytes)
            .and_then(|v| String::from_utf8(v).ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        *reference.borrow_mut() = Value::Int(cursor as i64);
        let values = reply.next().unwrap_or(redis::Value::Array(Vec::new()));
        let empty = matches!(&values, redis::Value::Array(v) if v.is_empty());
        if empty && cursor != 0 && options.scan & 1 != 0 {
            continue;
        }
        if empty {
            return Ok(Value::Bool(false));
        }
        let shape = match name {
            "hscan" => Shape::Pairs,
            "zscan" => Shape::Scores,
            "sscan" => Shape::Values,
            _ => Shape::Raw,
        };
        return crate::reply::convert(c, &options, &shape, values).await;
    }
}
