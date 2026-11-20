//! `sodium_crypto_pwhash_scryptsalsa208sha256_str($password, 524288, 16777216)`
//! returns a libsodium-compatible `$7$` password hash.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Value, args,
};
use scrypt::{Params, scrypt};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::{
    argument, exception, fixed,
    password::{off_thread, positive_limits},
    random, zeroed,
};

const RAW: &str = "sodium_crypto_pwhash_scryptsalsa208sha256";
const HASH: &str = "sodium_crypto_pwhash_scryptsalsa208sha256_str";
const ALPHABET: &[u8; 64] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const STRING_LENGTH: usize = 101;

fn raw(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let length = args::int(&args, 0).unwrap_or_default();
        if length <= 0 {
            return Err(argument(RAW, 1, "length", "must be greater than 0"));
        }
        let (ops, mem) = positive_limits(&args, RAW, 3)?;
        let password = password(context, &args, 1);
        let salt = *fixed::<32>(
            args::bytes(&args, 2),
            RAW,
            3,
            "salt",
            "SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_SALTBYTES",
        )?;
        let params = limits(RAW, 4, ops, mem)?;
        if length < 16 || length > i64::from(u32::MAX) * 32 {
            return Err(exception("internal error"));
        }
        let digest = off_thread(move || {
            let mut digest = zeroed(length as usize).map_err(|_| "arithmetic overflow")?;
            scrypt(&password, &salt, &params, &mut digest).map_err(|_| "internal error")?;
            Ok(digest)
        })
        .await?;
        Ok(Value::from(digest))
    })
}

fn hash(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let (ops, mem) = positive_limits(&args, HASH, 1)?;
        let password = password(context, &args, 0);
        let params = limits(HASH, 2, ops, mem)?;
        let hash = off_thread(move || {
            let mut setting = Vec::with_capacity(STRING_LENGTH);
            setting.extend_from_slice(b"$7$");
            append_integer(&mut setting, u32::from(params.log_n()), 1);
            append_integer(&mut setting, params.r(), 5);
            append_integer(&mut setting, params.p(), 5);
            let random = random::<32>().map_err(|_| "internal error")?;
            append_bytes(&mut setting, random.as_slice());
            let digest = digest(&password, &setting[14..], &params)?;
            setting.push(b'$');
            append_bytes(&mut setting, digest.as_slice());
            Ok(setting)
        })
        .await?;
        Ok(Value::from(hash))
    })
}

fn verify(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let password = password(context, &args, 1);
        let hash = args::bytes(&args, 0);
        if hash.len() != STRING_LENGTH {
            context.warning("wrong size for the hashed password".into());
            return Ok(Value::Bool(false));
        }
        let hash = hash.to_vec();
        let valid = off_thread(move || Ok(verify_hash(&password, &hash).unwrap_or(false))).await?;
        Ok(Value::Bool(valid))
    })
}

fn verify_hash(password: &[u8], hash: &[u8]) -> Option<bool> {
    let body = hash.strip_prefix(b"$7$")?;
    let n = read_integer(&body[..1])? as u8;
    let r = read_integer(&body[1..6])?;
    let p = read_integer(&body[6..11])?;
    let params = Params::new(n, r, p).ok()?;
    let (salt, expected) = body[11..].split_at(43);
    let expected = expected.strip_prefix(b"$")?;
    if salt.contains(&0) || salt.contains(&b'$') {
        return Some(false);
    }
    let digest = digest(password, salt, &params).ok()?;
    let mut encoded = Vec::with_capacity(43);
    append_bytes(&mut encoded, digest.as_slice());
    Some(bool::from(encoded.ct_eq(expected)))
}

fn password(context: &mut dyn Context, args: &[Value], index: usize) -> Zeroizing<Vec<u8>> {
    let password = args::bytes(args, index);
    if password.is_empty() {
        context.warning("empty password".into());
    }
    Zeroizing::new(password.to_vec())
}

fn limits(function: &str, position: usize, ops: i64, mem: i64) -> Result<Params, NativeError> {
    if ops < 524_288 {
        return Err(argument(
            function,
            position,
            "opslimit",
            "must be greater than or equal to 524288",
        ));
    }
    if mem < 16_777_216 {
        return Err(argument(
            function,
            position + 1,
            "memlimit",
            "must be greater than or equal to 16777216",
        ));
    }
    let (ops, mem) = (ops as u64, mem as u64);
    let (max_n, p) = if ops < mem / 32 {
        (ops / 32, 1)
    } else {
        let max_n = mem / 1024;
        let n = 1 << max_n.ilog2();
        (max_n, ((ops / 4 / n).min(0x3fff_ffff) / 8) as u32)
    };
    Params::new(max_n.ilog2() as u8, 8, p).map_err(|_| exception("internal error"))
}

fn digest(
    password: &[u8],
    salt: &[u8],
    params: &Params,
) -> Result<Zeroizing<[u8; 32]>, &'static str> {
    let mut digest = Zeroizing::new([0; 32]);
    scrypt(password, salt, params, digest.as_mut()).map_err(|_| "internal error")?;
    Ok(digest)
}

/// The crypt alphabet encodes the low six bits first.
fn append_integer(output: &mut Vec<u8>, mut value: u32, digits: usize) {
    for _ in 0..digits {
        output.push(ALPHABET[(value & 63) as usize]);
        value >>= 6;
    }
}

fn append_bytes(output: &mut Vec<u8>, bytes: &[u8]) {
    for chunk in bytes.chunks(3) {
        let mut input = [0; 4];
        input[..chunk.len()].copy_from_slice(chunk);
        append_integer(
            output,
            u32::from_le_bytes(input),
            (chunk.len() * 8).div_ceil(6),
        );
    }
}

fn read_integer(input: &[u8]) -> Option<u32> {
    let mut result = 0;
    for (position, byte) in input.iter().enumerate() {
        let digit = ALPHABET.iter().position(|candidate| candidate == byte)?;
        result |= (digit as u32) << (6 * position);
    }
    Some(result)
}

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::asynchronous(
            "sodium_crypto_pwhash_scryptsalsa208sha256",
            &[
                Param::new("length", ParamType::Int),
                Param::new("password", ParamType::String).sensitive(),
                Param::new("salt", ParamType::String),
                Param::new("opslimit", ParamType::Int),
                Param::new("memlimit", ParamType::Int),
            ],
            raw,
        ),
        "string",
    ),
    (
        FunctionEntry::asynchronous(
            "sodium_crypto_pwhash_scryptsalsa208sha256_str",
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
            "sodium_crypto_pwhash_scryptsalsa208sha256_str_verify",
            &[
                Param::new("hash", ParamType::String),
                Param::new("password", ParamType::String).sensitive(),
            ],
            verify,
        ),
        "bool",
    ),
];
