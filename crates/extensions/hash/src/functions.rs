//! One-shot hashing: `hash_hmac('sha256', 'message', 'key')` authenticates bytes.
use crate::{
    algorithms::{ALGORITHMS, Algorithm},
    options::{self, Options},
};
use phera_extension_api::{Context, NativeError, NativeResult, PhpArray, Value, args};
use subtle::ConstantTimeEq;

pub fn output(bytes: Vec<u8>, binary: bool) -> Value {
    if binary {
        Value::from(bytes)
    } else {
        Value::from(hex::encode(bytes))
    }
}
pub fn hash(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let algorithm = options::algorithm(args::bytes(arguments, 0), "hash")?;
    let options = Options::read(context, algorithm, arguments.get(3), "hash")?;
    Ok(output(
        algorithm.digest_with(&options, args::bytes(arguments, 1)),
        args::bool(arguments, 2).unwrap_or(false),
    ))
}
pub fn hmac(algorithm: Algorithm, data: &[u8], key: &[u8]) -> Vec<u8> {
    let size = algorithm
        .block_size()
        .expect("validated cryptographic hash");
    let key = if key.len() > size {
        algorithm.digest(key)
    } else {
        key.to_vec()
    };
    let mut inner = vec![0x36; size];
    let mut outer = vec![0x5c; size];
    for (i, byte) in key.iter().enumerate() {
        inner[i] ^= byte;
        outer[i] ^= byte;
    }
    inner.extend_from_slice(data);
    outer.extend_from_slice(&algorithm.digest(&inner));
    algorithm.digest(&outer)
}
pub fn hash_hmac(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let algorithm = options::cryptographic_algorithm(args::bytes(arguments, 0), "hash_hmac")?;
    Ok(output(
        hmac(
            algorithm,
            args::bytes(arguments, 1),
            args::bytes(arguments, 2),
        ),
        args::bool(arguments, 3).unwrap_or(false),
    ))
}
pub fn algos(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    list(ALGORITHMS.iter().map(|&(name, _)| name))
}
pub fn hmac_algos(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    list(
        ALGORITHMS
            .iter()
            .filter(|(_, algorithm)| algorithm.block_size().is_some())
            .map(|&(name, _)| name),
    )
}
fn list(names: impl Iterator<Item = &'static str>) -> NativeResult {
    let mut list = PhpArray::new();
    for name in names {
        list.push(Value::from(name))?;
    }
    Ok(Value::from(list))
}
pub fn equals(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    for (i, name) in ["known_string", "user_string"].iter().enumerate() {
        let value = arguments[i].deref();
        if !matches!(&*value, Value::String(_)) {
            let given = match &*value {
                Value::Bool(true) => "true".into(),
                Value::Bool(false) => "false".into(),
                value => value.type_name(),
            };
            return Err(NativeError::type_error(format!(
                "hash_equals(): Argument #{} (${name}) must be of type string, {given} given",
                i + 1
            )));
        }
    }

    Ok(Value::Bool(bool::from(
        args::bytes(arguments, 0).ct_eq(args::bytes(arguments, 1)),
    )))
}
pub fn md5(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(output(
        Algorithm::Md5.digest(args::bytes(arguments, 0)),
        args::bool(arguments, 1).unwrap_or(false),
    ))
}
pub fn sha1(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(output(
        Algorithm::Sha1.digest(args::bytes(arguments, 0)),
        args::bool(arguments, 1).unwrap_or(false),
    ))
}
pub fn pbkdf2(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let algorithm = options::cryptographic_algorithm(args::bytes(arguments, 0), "hash_pbkdf2")?;
    let iterations = args::int(arguments, 3).unwrap_or_default();
    let length = args::int(arguments, 4).unwrap_or_default();
    if iterations <= 0 {
        return Err(NativeError::value_error(
            "hash_pbkdf2(): Argument #4 ($iterations) must be greater than 0",
        ));
    }
    if length < 0 {
        return Err(NativeError::value_error(
            "hash_pbkdf2(): Argument #5 ($length) must be greater than or equal to 0",
        ));
    }
    // A cryptographic algorithm takes no seed, but PHP still checks the options.
    Options::read(context, algorithm, arguments.get(6), "hash_pbkdf2")?;
    let binary = args::bool(arguments, 5).unwrap_or(false);
    let size = algorithm.digest(&[]).len();
    let length = if length == 0 {
        size * if binary { 1 } else { 2 }
    } else {
        length as usize
    };
    let bytes = if binary { length } else { length.div_ceil(2) };
    let mut output = Vec::new();
    output
        .try_reserve(bytes)
        .map_err(|_| NativeError::error("Insufficient memory"))?;
    let password = args::bytes(arguments, 1);
    for index in 1..=bytes.div_ceil(size) {
        let mut input = args::bytes(arguments, 2).to_vec();
        input.extend_from_slice(&(index as u32).to_be_bytes());
        let mut block = hmac(algorithm, &input, password);
        let mut sum = block.clone();
        for _ in 1..iterations {
            block = hmac(algorithm, &block, password);
            for (a, b) in sum.iter_mut().zip(&block) {
                *a ^= b;
            }
        }
        output.extend_from_slice(&sum);
    }
    output.truncate(bytes);
    if binary {
        Ok(Value::from(output))
    } else {
        Ok(Value::from(hex::encode(output)[..length].to_string()))
    }
}
pub fn hkdf(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let algorithm = options::cryptographic_algorithm(args::bytes(arguments, 0), "hash_hkdf")?;
    let key = args::bytes(arguments, 1);
    if key.is_empty() {
        return Err(NativeError::value_error(
            "hash_hkdf(): Argument #2 ($key) must not be empty",
        ));
    }
    let length = args::int(arguments, 2).unwrap_or_default();
    if length < 0 {
        return Err(NativeError::value_error(
            "hash_hkdf(): Argument #3 ($length) must be greater than or equal to 0",
        ));
    }
    let size = algorithm.digest(&[]).len();
    if length as usize > size * 255 {
        return Err(NativeError::value_error(format!(
            "hash_hkdf(): Argument #3 ($length) must be less than or equal to {}",
            size * 255
        )));
    }
    let length = if length == 0 { size } else { length as usize };
    let extracted = hmac(algorithm, key, args::bytes(arguments, 4));
    let mut result = Vec::with_capacity(length);
    let mut previous = Vec::new();
    for i in 1..=length.div_ceil(size) {
        previous.extend_from_slice(args::bytes(arguments, 3));
        previous.push(i as u8);
        previous = hmac(algorithm, &previous, &extracted);
        result.extend_from_slice(&previous);
    }
    result.truncate(length);
    Ok(Value::from(result))
}
