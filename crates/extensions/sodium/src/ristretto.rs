//! `sodium_crypto_core_ristretto255_from_hash($hash)` maps 64 bytes to a group element.

use curve25519_dalek::{
    ristretto::{CompressedRistretto, RistrettoPoint},
    scalar::Scalar,
    traits::IsIdentity,
};
use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

use crate::{argument, exception, fixed, random};

const CORE_BYTES: &str = "SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES";
const MULT_BYTES: &str = "SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_BYTES";
const MULT_SCALAR: &str = "SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_SCALARBYTES";

fn point(value: RistrettoPoint) -> Value {
    Value::from(value.compress().as_bytes().as_slice())
}

fn from_hash(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let hash = fixed::<64>(
        args::bytes(args, 0),
        "sodium_crypto_core_ristretto255_from_hash",
        1,
        "s",
        "SODIUM_CRYPTO_CORE_RISTRETTO255_HASHBYTES",
    )?;
    Ok(point(RistrettoPoint::from_uniform_bytes(hash)))
}

fn valid(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let bytes = fixed::<32>(
        args::bytes(args, 0),
        "sodium_crypto_core_ristretto255_is_valid_point",
        1,
        "s",
        CORE_BYTES,
    )?;
    Ok(Value::Bool(
        CompressedRistretto(*bytes).decompress().is_some(),
    ))
}

fn random_point(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(point(RistrettoPoint::from_uniform_bytes(&*random::<64>()?)))
}

fn add(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    combine(args, "sodium_crypto_core_ristretto255_add", |p, q| p + q)
}

fn sub(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    combine(args, "sodium_crypto_core_ristretto255_sub", |p, q| p - q)
}

fn combine(
    args: &[Value],
    function: &str,
    operation: fn(RistrettoPoint, RistrettoPoint) -> RistrettoPoint,
) -> NativeResult {
    let p = fixed::<32>(args::bytes(args, 0), function, 1, "p", CORE_BYTES)?;
    let q = fixed::<32>(args::bytes(args, 1), function, 2, "q", CORE_BYTES)?;
    let p = CompressedRistretto(*p)
        .decompress()
        .ok_or_else(|| exception("internal error"))?;
    let q = CompressedRistretto(*q)
        .decompress()
        .ok_or_else(|| exception("internal error"))?;
    Ok(point(operation(p, q)))
}

fn multiply(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let function = "sodium_crypto_scalarmult_ristretto255";
    let n = fixed::<32>(args::bytes(args, 0), function, 1, "n", MULT_SCALAR)?;
    let p = fixed::<32>(args::bytes(args, 1), function, 2, "p", MULT_BYTES)?;
    let result = CompressedRistretto(*p)
        .decompress()
        .map(|point| point * multiplier(n))
        .filter(|point| !point.is_identity())
        .ok_or_else(|| exception("Result is identity element"))?;
    Ok(point(result))
}

fn base(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let function = "sodium_crypto_scalarmult_ristretto255_base";
    let n = fixed::<32>(args::bytes(args, 0), function, 1, "n", MULT_SCALAR)?;
    let result = RistrettoPoint::mul_base(&multiplier(n));
    if result.is_identity() {
        return Err(argument(function, 1, "n", "must not be zero"));
    }
    Ok(point(result))
}

fn multiplier(bytes: &[u8; 32]) -> Scalar {
    let mut bytes = zeroize::Zeroizing::new(*bytes);
    // Sodium's Ristretto scalar multiplication ignores bit 255.
    bytes[31] &= 0x7f;
    Scalar::from_bytes_mod_order(*bytes)
}

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync(
            "sodium_crypto_core_ristretto255_from_hash",
            &[Param::new("s", ParamType::String)],
            from_hash,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_core_ristretto255_is_valid_point",
            &[Param::new("s", ParamType::String)],
            valid,
        ),
        "bool",
    ),
    (
        FunctionEntry::sync("sodium_crypto_core_ristretto255_random", &[], random_point),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_core_ristretto255_add",
            &[
                Param::new("p", ParamType::String),
                Param::new("q", ParamType::String),
            ],
            add,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_core_ristretto255_sub",
            &[
                Param::new("p", ParamType::String),
                Param::new("q", ParamType::String),
            ],
            sub,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_scalarmult_ristretto255",
            &[
                Param::new("n", ParamType::String),
                Param::new("p", ParamType::String),
            ],
            multiply,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_scalarmult_ristretto255_base",
            &[Param::new("n", ParamType::String)],
            base,
        ),
        "string",
    ),
];
