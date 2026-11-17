//! What a stored password hash says about itself:
//!
//! ```php
//! password_get_info('$2y$10$...');
//! // ['algo' => '2y', 'algoName' => 'bcrypt', 'options' => ['cost' => 10]]
//! password_needs_rehash('$2y$10$...', PASSWORD_BCRYPT, ['cost' => 12]);  // true
//! password_algos();  // ['2y', 'argon2i', 'argon2id']
//! ```

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Value, args,
};

use crate::password_options::{Algorithm, Options};

pub const FUNCTIONS: &[FunctionEntry] = &[PASSWORD_GET_INFO, PASSWORD_NEEDS_REHASH, PASSWORD_ALGOS];

/// The length of every bcrypt hash: `$2y$`, two digits of cost, `$`, and
/// 53 characters of salt and digest.
const BCRYPT_LENGTH: usize = 60;

/// The algorithm a stored hash was made with, if it is one of PHP's.
pub fn identify(hash: &[u8]) -> Option<Algorithm> {
    if hash.starts_with(b"$2y$") && hash.len() == BCRYPT_LENGTH && hash[6] == b'$' {
        return bcrypt_cost(hash).map(|_| Algorithm::Bcrypt);
    }
    // `$argon2id$v=19$...`: the name runs from the first `$` to the next.
    let rest = hash.get(1..)?;
    let end = rest.iter().position(|&byte| byte == b'$')?;
    match &rest[..end] {
        b"argon2i" => Some(Algorithm::Argon2i),
        b"argon2id" => Some(Algorithm::Argon2id),
        _ => None,
    }
}

/// The two digits after `$2y$`, if they are a cost bcrypt allows.
fn bcrypt_cost(hash: &[u8]) -> Option<i64> {
    let [tens, ones] = [hash[4], hash[5]];
    if !tens.is_ascii_digit() || !ones.is_ascii_digit() {
        return None;
    }
    let cost = i64::from((tens - b'0') * 10 + (ones - b'0'));
    (4..=31).contains(&cost).then_some(cost)
}

/// `options` with what `hash` records about its costs put in. An Argon2
/// hash records them as `$v=19$m=65536,t=4,p=1$`; reading stops at the
/// first part that is not there.
fn stored(hash: &[u8], mut options: Options) -> Options {
    if identify(hash) == Some(Algorithm::Bcrypt) {
        options.cost = bcrypt_cost(hash).expect("identify checked the cost");
        return options;
    }
    let text = String::from_utf8_lossy(hash);
    let Some(mut rest) = text
        .strip_prefix("$argon2i$")
        .or_else(|| text.strip_prefix("$argon2id$"))
    else {
        return options;
    };
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
        // Like C's strtol, which PHP uses: a sign and digits.
        let length = tail
            .bytes()
            .take_while(|byte| byte.is_ascii_digit() || *byte == b'-' || *byte == b'+')
            .count();
        let Ok(value) = tail[..length].parse() else {
            break;
        };
        *slot = value;
        rest = &tail[length..];
    }
    options
}

/// `password_get_info(string $hash): array`
const PASSWORD_GET_INFO: FunctionEntry = FunctionEntry::sync(
    "password_get_info",
    &[Param::new("hash", ParamType::String)],
    password_get_info,
);

fn password_get_info(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let hash = args::bytes(arguments, 0);
    let algorithm = identify(hash);
    let options = stored(hash, Options::default());
    let mut recorded = PhpArray::new();
    match algorithm {
        Some(Algorithm::Bcrypt) => {
            recorded.insert(ArrayKey::str("cost"), Value::Int(options.cost));
        }
        Some(Algorithm::Argon2i | Algorithm::Argon2id) => {
            recorded.insert(ArrayKey::str("memory_cost"), Value::Int(options.memory));
            recorded.insert(ArrayKey::str("time_cost"), Value::Int(options.time));
            recorded.insert(ArrayKey::str("threads"), Value::Int(options.threads));
        }
        None => {}
    }
    let mut info = PhpArray::new();
    let id = algorithm.map_or(Value::Null, |algorithm| Value::from(algorithm.id()));
    info.insert(ArrayKey::str("algo"), id);
    let name = algorithm.map_or("unknown", Algorithm::name);
    info.insert(ArrayKey::str("algoName"), Value::from(name));
    info.insert(ArrayKey::str("options"), Value::from(recorded));
    Ok(Value::from(info))
}

/// `password_needs_rehash(string $hash, string|int|null $algo, array $options = []): bool`
const PASSWORD_NEEDS_REHASH: FunctionEntry = FunctionEntry::sync(
    "password_needs_rehash",
    &[
        Param::new("hash", ParamType::String),
        Param::new("algo", ParamType::IntOrString).nullable(),
        Param::new("options", ParamType::Array).optional(),
    ],
    password_needs_rehash,
);

fn password_needs_rehash(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    // Unlike password_hash(), an unknown algorithm is not an error.
    let Some(algorithm) = Algorithm::parse(args::value(arguments, 1)) else {
        return Ok(Value::Bool(false));
    };
    let hash = args::bytes(arguments, 0);
    if identify(hash) != Some(algorithm) {
        return Ok(Value::Bool(true));
    }
    let wanted = Options::read(context, arguments, algorithm)?;
    // A cost the hash does not record counts as 0, which never matches.
    let unrecorded = Options {
        cost: 0,
        memory: 0,
        time: 0,
        threads: 0,
    };
    let recorded = stored(hash, unrecorded);
    let differs = match algorithm {
        Algorithm::Bcrypt => recorded.cost != wanted.cost,
        Algorithm::Argon2i | Algorithm::Argon2id => {
            (recorded.memory, recorded.time, recorded.threads)
                != (wanted.memory, wanted.time, wanted.threads)
        }
    };
    Ok(Value::Bool(differs))
}

/// `password_algos(): array`
const PASSWORD_ALGOS: FunctionEntry = FunctionEntry::sync("password_algos", &[], password_algos);

fn password_algos(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    let mut algorithms = PhpArray::new();
    for algorithm in [Algorithm::Bcrypt, Algorithm::Argon2i, Algorithm::Argon2id] {
        algorithms.push(Value::from(algorithm.id()))?;
    }
    Ok(Value::from(algorithms))
}
