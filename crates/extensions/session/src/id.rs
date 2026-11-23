//! `session_create_id('app-')` prefixes 32 random lower-case hexadecimal digits.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, PhpString,
    Value, args,
};

use crate::{
    settings,
    state::{can_change, state},
};

pub const MAX_ID: usize = 256;

pub const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::sync(
        "session_id",
        &[Param::new("id", ParamType::String).nullable().optional()],
        session_id,
    ),
    FunctionEntry::asynchronous(
        "session_create_id",
        &[Param::new("prefix", ParamType::String).optional()],
        create,
    ),
];

pub fn valid(bytes: &[u8]) -> bool {
    (1..=MAX_ID).contains(&bytes.len())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b','))
}

pub fn random() -> Result<PhpString, NativeError> {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| {
        NativeError::throw("Random\\RandomException", "Failed to generate random bytes")
    })?;
    let mut id = Vec::with_capacity(32);
    for byte in bytes {
        id.push(DIGITS[usize::from(byte & 15)]);
        id.push(DIGITS[usize::from(byte >> 4)]);
    }
    Ok(id.into())
}

fn session_id(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let next = args::string(arguments, 0);
    let cookies = settings::boolean(context, "use_cookies");
    if next.is_some() && !can_change(context, "session_id", "ID", cookies) {
        return Ok(Value::Bool(false));
    }
    let id = &mut state(context).id;
    let previous = Value::from(
        id.as_ref()
            .map_or(b"".as_slice(), |id| c_string(id.as_bytes()))
            .to_vec(),
    );
    if let Some(next) = next {
        *id = Some(next.clone());
    }
    Ok(previous)
}

pub fn c_string(bytes: &[u8]) -> &[u8] {
    bytes.split(|byte| *byte == 0).next().unwrap_or_default()
}

fn create(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(create_prefixed(context, arguments))
}

async fn create_prefixed(context: &mut dyn Context, arguments: Vec<Value>) -> NativeResult {
    let prefix = args::bytes(&arguments, 0);
    let label = "session_create_id(): Argument #1 ($prefix)";
    if prefix.contains(&0) {
        return Err(NativeError::value_error(format!(
            "{label} must not contain any null bytes"
        )));
    }
    if prefix.len() > MAX_ID {
        return Err(NativeError::value_error(format!(
            "{label} cannot be longer than 256 characters"
        )));
    }
    if !prefix.is_empty() && !valid(prefix) {
        context.warning(
            "session_create_id(): Prefix cannot contain special characters. Only the A-Z, a-z, \
             0-9, \"-\", and \",\" characters are allowed"
                .into(),
        );
        return Ok(Value::Bool(false));
    }
    let mut result = prefix.to_vec();
    let function = "session_create_id";
    if !crate::state::active(context) || state(context).in_handler {
        result.extend_from_slice(random()?.as_bytes());
        return Ok(Value::from(result));
    }
    for _ in 0..3 {
        let id = crate::user_handler::create(context, function).await?;
        if !crate::storage::validates(context)
            || !crate::storage::validate(context, &id, function).await?
        {
            result.extend_from_slice(id.as_bytes());
            return Ok(Value::from(result));
        }
    }
    context.warning(format!("{function}(): Failed to create new ID"));
    Ok(Value::Bool(false))
}
