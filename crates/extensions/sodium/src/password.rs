//! `sodium_crypto_pwhash_str($password, 2, 67108864)` creates an Argon2id PHC string.

use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::SaltString,
};
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, Value, args,
};
use zeroize::Zeroizing;

use crate::{argument, exception, fixed, random};

const RAW: &str = "sodium_crypto_pwhash";
const HASH: &str = "sodium_crypto_pwhash_str";
const VERIFY: &str = "sodium_crypto_pwhash_str_verify";

pub(crate) async fn off_thread<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, &'static str> + Send + 'static,
) -> Result<T, NativeError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| exception("internal error"))?
        .map_err(exception)
}

fn raw(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let size = args::int(&args, 0).unwrap_or_default();
        if size <= 0 {
            return Err(argument(RAW, 1, "length", "must be greater than 0"));
        }
        if size >= i64::from(u32::MAX) {
            return Err(argument(RAW, 1, "length", "is too large"));
        }
        let (ops, mem) = positive_limits(&args, RAW, 3)?;
        let algorithm = match args::int(&args, 5).unwrap_or(2) {
            1 => Algorithm::Argon2i,
            2 => Algorithm::Argon2id,
            _ => return Err(exception("unsupported password hashing algorithm")),
        };
        let password = password(context, &args, RAW, 1)?;
        let salt = *fixed::<16>(
            args::bytes(&args, 2),
            RAW,
            3,
            "salt",
            "SODIUM_CRYPTO_PWHASH_SALTBYTES",
        )?;
        let params = parameters(RAW, 4, ops, mem, size as usize, algorithm)?;
        let hash = off_thread(move || {
            let mut hash = crate::zeroed(size as usize).map_err(|_| "arithmetic overflow")?;
            Argon2::new(algorithm, Version::V0x13, params)
                .hash_password_into(&password, &salt, &mut hash)
                .map_err(|_| "internal error")?;
            Ok(hash)
        })
        .await?;
        Ok(Value::from(hash))
    })
}

fn hash(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let (ops, mem) = positive_limits(&args, HASH, 1)?;
        let password = password(context, &args, HASH, 0)?;
        let params = parameters(HASH, 2, ops, mem, 32, Algorithm::Argon2id)?;
        let hash = off_thread(move || {
            let salt =
                SaltString::encode_b64(random::<16>().map_err(|_| "internal error")?.as_slice())
                    .map_err(|_| "internal error")?;
            Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
                .hash_password(&password, &salt)
                .map(|hash| hash.to_string())
                .map_err(|_| "internal error")
        })
        .await?;
        Ok(Value::from(hash))
    })
}

fn verify(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let password = password(context, &args, VERIFY, 1)?;
        let hash = c_string(args::bytes(&args, 0)).to_vec();
        let valid = off_thread(move || {
            Ok(load_hash(&hash)
                .is_some_and(|hash| Argon2::default().verify_password(&password, &hash).is_ok()))
        })
        .await?;
        Ok(Value::Bool(valid))
    })
}

fn needs_rehash(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let ops = args::int(args, 1).unwrap_or_default();
    let mem = args::int(args, 2).unwrap_or_default();
    let matches = load_hash(c_string(args::bytes(args, 0))).is_some_and(|hash| {
        hash.params.get_decimal("t").map(i64::from) == Some(ops)
            && hash.params.get_decimal("m").map(i64::from) == Some(mem / 1024)
    });
    Ok(Value::Bool(!matches))
}

fn load_hash(input: &[u8]) -> Option<PasswordHash<'_>> {
    if input.len() >= 128 {
        return None;
    }
    let hash = PasswordHash::new(std::str::from_utf8(input).ok()?).ok()?;
    match hash.algorithm.as_str() {
        "argon2i" | "argon2id" if hash.salt.is_some() && hash.hash.is_some() => Some(hash),
        _ => None,
    }
}

pub(crate) fn c_string(bytes: &[u8]) -> &[u8] {
    bytes.split(|&byte| byte == 0).next().unwrap_or_default()
}

pub(crate) fn positive_limits(
    args: &[Value],
    function: &str,
    index: usize,
) -> Result<(i64, i64), NativeError> {
    let ops = args::int(args, index).unwrap_or_default();
    let mem = args::int(args, index + 1).unwrap_or_default();
    if ops <= 0 {
        return Err(argument(
            function,
            index + 1,
            "opslimit",
            "must be greater than 0",
        ));
    }
    if mem <= 0 {
        return Err(argument(
            function,
            index + 2,
            "memlimit",
            "must be greater than 0",
        ));
    }
    Ok((ops, mem))
}

fn password(
    context: &mut dyn Context,
    args: &[Value],
    function: &str,
    index: usize,
) -> Result<Zeroizing<Vec<u8>>, NativeError> {
    let password = args::bytes(args, index);
    if password.len() >= u32::MAX as usize {
        return Err(argument(function, index + 1, "password", "is too long"));
    }
    if password.is_empty() {
        context.warning("empty password".into());
    }
    Ok(Zeroizing::new(password.to_vec()))
}

fn parameters(
    function: &str,
    position: usize,
    ops: i64,
    mem: i64,
    size: usize,
    algorithm: Algorithm,
) -> Result<Params, NativeError> {
    if ops < 1 {
        return Err(argument(
            function,
            position,
            "opslimit",
            "must be greater than or equal to 1",
        ));
    }
    if mem < 8192 {
        return Err(argument(
            function,
            position + 1,
            "memlimit",
            "must be greater than or equal to 8192",
        ));
    }
    if size < 16 || (algorithm == Algorithm::Argon2i && ops < 3) {
        return Err(exception("internal error"));
    }
    let ops = u32::try_from(ops).map_err(|_| exception("internal error"))?;
    let mem = u32::try_from(mem / 1024).map_err(|_| exception("internal error"))?;
    Params::new(mem, ops, 1, Some(size)).map_err(|_| exception("internal error"))
}

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::asynchronous(
            "sodium_crypto_pwhash",
            &[
                Param::new("length", ParamType::Int),
                Param::new("password", ParamType::String).sensitive(),
                Param::new("salt", ParamType::String),
                Param::new("opslimit", ParamType::Int),
                Param::new("memlimit", ParamType::Int),
                Param::new("algo", ParamType::Int).with_constant_default(
                    "SODIUM_CRYPTO_PWHASH_ALG_DEFAULT",
                    ConstantValue::Int(2),
                ),
            ],
            raw,
        ),
        "string",
    ),
    (
        FunctionEntry::asynchronous(
            "sodium_crypto_pwhash_str",
            &[
                Param::new("password", ParamType::String).sensitive(),
                Param::new("opslimit", ParamType::Int),
                Param::new("memlimit", ParamType::Int),
            ],
            hash,
        ),
        "string",
    ),
    (
        FunctionEntry::asynchronous(
            "sodium_crypto_pwhash_str_verify",
            &[
                Param::new("hash", ParamType::String),
                Param::new("password", ParamType::String).sensitive(),
            ],
            verify,
        ),
        "bool",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_pwhash_str_needs_rehash",
            &[
                Param::new("password", ParamType::String),
                Param::new("opslimit", ParamType::Int),
                Param::new("memlimit", ParamType::Int),
            ],
            needs_rehash,
        ),
        "bool",
    ),
];
