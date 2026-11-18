//! Stream replies preserve entry ids and field names instead of exposing RESP tuples.
use super::*;

pub async fn entries(c: &mut dyn Context, o: &Options, reply: Reply) -> NativeResult {
    let mut result = PhpArray::new();
    for entry in items(reply) {
        let mut entry = items(entry).into_iter();
        if let (Some(id), Some(fields)) = (entry.next(), entry.next()) {
            result.insert(key(bytes(id))?, pairs(c, o, fields, false, true).await?);
        }
    }
    Ok(Value::from(result))
}

pub async fn streams(c: &mut dyn Context, o: &Options, reply: Reply) -> NativeResult {
    let mut result = PhpArray::new();
    let streams = match reply {
        Reply::Map(pairs) => pairs
            .into_iter()
            .map(|(k, v)| Reply::Array(vec![k, v]))
            .collect(),
        other => items(other),
    };
    for stream in streams {
        let mut stream = items(stream).into_iter();
        if let (Some(name), Some(values)) = (stream.next(), stream.next()) {
            result.insert(key(bytes(name))?, entries(c, o, values).await?);
        }
    }
    Ok(Value::from(result))
}

pub async fn claim(c: &mut dyn Context, o: &Options, reply: Reply, just_id: bool) -> NativeResult {
    let mut result = PhpArray::new();
    for (i, item) in items(reply).into_iter().enumerate() {
        result.push(if i == 1 && !just_id {
            entries(c, o, item).await?
        } else {
            raw::convert(item, o)?
        })?;
    }
    Ok(Value::from(result))
}

pub async fn info(c: &mut dyn Context, o: &Options, reply: Reply) -> NativeResult {
    let mut result = PhpArray::new();
    let mut values = items(reply).into_iter();
    while let (Some(k), Some(v)) = (values.next(), values.next()) {
        let name = bytes(k);
        let v = if name == b"first-entry" || name == b"last-entry" {
            entries(c, o, Reply::Array(vec![v])).await?
        } else {
            raw::convert(v, o)?
        };
        result.insert(key(name)?, v);
    }
    Ok(Value::from(result))
}
