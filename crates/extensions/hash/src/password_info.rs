//! `password_get_info($hash)` reports the stored algorithm and rehash options.
use crate::password_options::{Algorithm, Options};
use phera_extension_api::{ArrayKey, Context, NativeResult, PhpArray, Value, args};

pub fn identify(hash: &[u8]) -> Option<Algorithm> {
    if hash.starts_with(b"$2y$")
        && hash.len() == 60
        && hash[6] == b'$'
        && hash[4].is_ascii_digit()
        && hash[5].is_ascii_digit()
        && (4..=31).contains(&((hash[4] - b'0') * 10 + hash[5] - b'0'))
    {
        return Some(Algorithm::Bcrypt);
    }
    let identifier = hash.get(1..)?.split(|b| *b == b'$').next()?;
    if !hash.get(1..)?.contains(&b'$') {
        return None;
    }
    match identifier {
        b"argon2i" => Some(Algorithm::Argon2i),
        b"argon2id" => Some(Algorithm::Argon2id),
        _ => None,
    }
}
fn stored(hash: &[u8], mut options: Options) -> Options {
    if identify(hash) == Some(Algorithm::Bcrypt) {
        options.cost = i64::from((hash[4] - b'0') * 10 + hash[5] - b'0');
        return options;
    }
    let text = String::from_utf8_lossy(hash);
    let Some(rest) = text
        .strip_prefix("$argon2i$")
        .or_else(|| text.strip_prefix("$argon2id$"))
    else {
        return options;
    };
    let mut rest = rest;
    let mut version = 0;
    for (prefix, slot) in [
        ("v=", &mut version),
        ("$m=", &mut options.memory),
        (",t=", &mut options.time),
        (",p=", &mut options.threads),
    ] {
        let Some(tail) = rest.strip_prefix(prefix) else {
            break;
        };
        let length = tail
            .bytes()
            .take_while(|b| b.is_ascii_digit() || *b == b'-' || *b == b'+')
            .count();
        let Ok(value) = tail[..length].parse() else {
            break;
        };
        *slot = value;
        rest = &tail[length..];
    }
    options
}
pub fn info(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let hash = args::bytes(arguments, 0);
    let algorithm = identify(hash);
    let options = stored(hash, Options::default());
    let mut values = PhpArray::new();
    match algorithm {
        Some(Algorithm::Bcrypt) => {
            values.insert(ArrayKey::str("cost"), Value::Int(options.cost));
        }
        Some(_) => {
            for (name, value) in [
                ("memory_cost", options.memory),
                ("time_cost", options.time),
                ("threads", options.threads),
            ] {
                values.insert(ArrayKey::str(name), Value::Int(value));
            }
        }
        None => {}
    }
    let mut result = PhpArray::new();
    result.insert(
        ArrayKey::str("algo"),
        algorithm.map_or(Value::Null, |a| Value::from(a.id())),
    );
    result.insert(
        ArrayKey::str("algoName"),
        Value::from(algorithm.map_or("unknown", Algorithm::name)),
    );
    result.insert(ArrayKey::str("options"), Value::from(values));
    Ok(Value::from(result))
}
pub fn needs_rehash(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let Some(algorithm) = Algorithm::parse(arguments.get(1)) else {
        return Ok(Value::Bool(false));
    };
    let hash = args::bytes(arguments, 0);
    if identify(hash) != Some(algorithm) {
        return Ok(Value::Bool(true));
    }
    let requested = Options::read(context, arguments, algorithm)?;
    let old = stored(
        hash,
        Options {
            cost: 0,
            memory: 0,
            time: 0,
            threads: 0,
        },
    );
    Ok(Value::Bool(if algorithm == Algorithm::Bcrypt {
        old.cost != requested.cost
    } else {
        old.memory != requested.memory
            || old.time != requested.time
            || old.threads != requested.threads
    }))
}
pub fn algos(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    for name in ["2y", "argon2i", "argon2id"] {
        result.push(Value::from(name))?;
    }
    Ok(Value::from(result))
}
