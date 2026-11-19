//! `openssl_random_pseudo_bytes(32, $strong)` uses the OS RNG and sets $strong.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

pub(crate) const ENTRY: FunctionEntry = FunctionEntry::sync(
    "openssl_random_pseudo_bytes",
    &[
        Param::new("length", ParamType::Int),
        Param::new("strong_result", ParamType::Mixed)
            .by_ref()
            .optional(),
    ],
    random,
);

fn random(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let length = args::int(arguments, 0).unwrap_or_default();
    let result = random_bytes(length);
    if let Some(output) = args::reference(arguments, 1) {
        *output.borrow_mut() = Value::Bool(result.is_ok());
    }
    result.map(Value::from)
}

fn random_bytes(length: i64) -> Result<Vec<u8>, NativeError> {
    let prefix = "openssl_random_pseudo_bytes(): Argument #1 ($length)";
    if length <= 0 {
        return Err(NativeError::value_error(format!(
            "{prefix} must be greater than 0"
        )));
    }
    let length = i32::try_from(length).map_err(|_| {
        NativeError::value_error(format!(
            "{prefix} must be less than or equal to {}",
            i32::MAX
        ))
    })? as usize;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length).map_err(|_| rng_error())?;
    bytes.resize(length, 0);
    getrandom::fill(&mut bytes).map_err(|_| rng_error())?;
    Ok(bytes)
}

fn rng_error() -> NativeError {
    NativeError::throw("Exception", "Error reading from source device")
}
