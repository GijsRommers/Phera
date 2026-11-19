//! `sodium_crypto_core_ristretto255_scalar_reduce($bytes)` reduces a 64-byte integer.

use curve25519_dalek::scalar::Scalar;
use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};
use zeroize::Zeroizing;

use crate::{exception, fixed, random};

fn output(value: Scalar) -> Value {
    Value::from(value.as_bytes().as_slice())
}

fn add(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    binary(
        args,
        "sodium_crypto_core_ristretto255_scalar_add",
        |x, y| x + y,
    )
}

fn sub(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    binary(
        args,
        "sodium_crypto_core_ristretto255_scalar_sub",
        |x, y| x - y,
    )
}

fn mul(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    binary(
        args,
        "sodium_crypto_core_ristretto255_scalar_mul",
        |x, y| x * y,
    )
}

fn read(
    args: &[Value],
    function: &str,
    position: usize,
    name: &str,
) -> Result<Zeroizing<Scalar>, phera_extension_api::NativeError> {
    let bytes = fixed::<32>(
        args::bytes(args, position - 1),
        function,
        position,
        name,
        "SODIUM_CRYPTO_CORE_RISTRETTO255_SCALARBYTES",
    )?;
    Ok(Zeroizing::new(Scalar::from_bytes_mod_order(*bytes)))
}

fn binary(
    args: &[Value],
    function: &str,
    operation: fn(&Scalar, &Scalar) -> Scalar,
) -> NativeResult {
    let x = read(args, function, 1, "x")?;
    let y = read(args, function, 2, "y")?;
    Ok(output(operation(&x, &y)))
}

fn complement(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let s = read(
        args,
        "sodium_crypto_core_ristretto255_scalar_complement",
        1,
        "s",
    )?;
    Ok(output(Scalar::ONE - *s))
}

fn invert(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let s = read(
        args,
        "sodium_crypto_core_ristretto255_scalar_invert",
        1,
        "s",
    )?;
    if *s == Scalar::ZERO {
        return Err(exception("internal error"));
    }
    Ok(output(s.invert()))
}

fn negate(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let s = read(
        args,
        "sodium_crypto_core_ristretto255_scalar_negate",
        1,
        "s",
    )?;
    Ok(output(-*s))
}

fn random_scalar(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    loop {
        let mut bytes = random::<32>()?;
        bytes[31] &= 0x1f;
        if let Some(s) = Option::<Scalar>::from(Scalar::from_canonical_bytes(*bytes))
            && s != Scalar::ZERO
        {
            return Ok(output(s));
        }
    }
}

fn reduce(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let bytes = fixed::<64>(
        args::bytes(args, 0),
        "sodium_crypto_core_ristretto255_scalar_reduce",
        1,
        "s",
        "SODIUM_CRYPTO_CORE_RISTRETTO255_NONREDUCEDSCALARBYTES",
    )?;
    Ok(output(Scalar::from_bytes_mod_order_wide(bytes)))
}

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync(
            "sodium_crypto_core_ristretto255_scalar_add",
            &[
                Param::new("x", ParamType::String),
                Param::new("y", ParamType::String),
            ],
            add,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_core_ristretto255_scalar_sub",
            &[
                Param::new("x", ParamType::String),
                Param::new("y", ParamType::String),
            ],
            sub,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_core_ristretto255_scalar_mul",
            &[
                Param::new("x", ParamType::String),
                Param::new("y", ParamType::String),
            ],
            mul,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_core_ristretto255_scalar_complement",
            &[Param::new("s", ParamType::String)],
            complement,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_core_ristretto255_scalar_invert",
            &[Param::new("s", ParamType::String)],
            invert,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_core_ristretto255_scalar_negate",
            &[Param::new("s", ParamType::String)],
            negate,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_core_ristretto255_scalar_random",
            &[],
            random_scalar,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_core_ristretto255_scalar_reduce",
            &[Param::new("s", ParamType::String)],
            reduce,
        ),
        "string",
    ),
];
