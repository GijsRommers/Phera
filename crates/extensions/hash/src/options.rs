//! Hash options are algorithm-specific: `hash('xxh3', 'abc', false, ['seed'=>42])`.
use crate::algorithms::Algorithm;
use phera_extension_api::{ArrayKey, Context, NativeError, Value, args, value::to_php_string};

pub fn algorithm(
    arguments: &[Value],
    function: &str,
    crypto: bool,
) -> Result<Algorithm, NativeError> {
    let algorithm = Algorithm::find(args::bytes(arguments, 0));
    match algorithm {
        Some(a) if !crypto || a.block_size().is_some() => Ok(a),
        _ => Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($algo) must be a valid {}hashing algorithm",
            if crypto { "cryptographic " } else { "" }
        ))),
    }
}
pub fn apply(
    context: &mut dyn Context,
    algorithm: &mut Algorithm,
    options: Option<&Value>,
    function: &str,
) -> Result<(), NativeError> {
    let Some(Value::Array(options)) = options else {
        return Ok(());
    };
    if !algorithm.name.starts_with("murmur") && !algorithm.name.starts_with("xxh") {
        return Ok(());
    }
    let seed = options.get(&ArrayKey::str("seed")).map(Value::deref);
    let secret = options.get(&ArrayKey::str("secret")).map(Value::deref);
    let xx3 = matches!(algorithm.name, "xxh3" | "xxh128");
    if xx3 && seed.is_some() && secret.is_some() {
        return Err(NativeError::error(format!(
            "{}: Only one of seed or secret is to be passed for initialization",
            algorithm.name
        )));
    }
    if let Some(seed) = seed {
        match &*seed {
            Value::Int(seed) => algorithm.seed = *seed as u64,
            _ => context.deprecated(format!(
                "{function}(): Passing a seed of a type other \
                than int is deprecated because it is {}",
                if xx3 {
                    "ignored"
                } else {
                    "the same as setting the seed to 0"
                }
            )),
        }
    }
    if xx3 && let Some(secret) = secret {
        if !matches!(&*secret, Value::String(_)) {
            context.deprecated(format!(
                "{function}(): Passing a secret of a type other than \
                string is deprecated because it \
            implicitly converts to a string, potentially hiding bugs"
            ));
        }
        let secret = to_php_string(&secret, context.diagnostics())?;
        let mut bytes = secret.as_bytes().to_vec();
        if bytes.len() < 136 {
            return Err(NativeError::error(format!(
                "{}: Secret length must be >= 136 bytes, {} bytes passed",
                algorithm.name,
                bytes.len()
            )));
        }
        if bytes.len() > 256 {
            context.warning(format!(
                "{function}(): {}: Secret content exceeding 256 bytes discarded",
                algorithm.name
            ));
            bytes.truncate(256);
        }
        algorithm.secret = Some(bytes);
    }
    Ok(())
}
