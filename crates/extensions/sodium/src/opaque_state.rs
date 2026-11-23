//! A copied PHP state string, such as `$copy = $state`, can be resumed independently.

use phera_extension_api::{NativeError, NativeResult, Value, args};
use zeroize::{Zeroize, Zeroizing};

use crate::argument;

pub(crate) fn with_state<const N: usize>(
    args: &[Value],
    function: &str,
    invalid_state: impl Fn() -> NativeError,
    operation: impl FnOnce(&mut Value, &[u8; N]) -> NativeResult,
) -> NativeResult {
    let type_error = || argument(function, 1, "state", "must be a reference to a state");
    let mut variable = args::reference(args, 0)
        .ok_or_else(type_error)?
        .borrow_mut();
    let Some(bytes) = variable.as_str_bytes() else {
        return Err(type_error());
    };
    let bytes: &[u8; N] = bytes.try_into().map_err(|_| invalid_state())?;
    let bytes = Zeroizing::new(*bytes);
    operation(&mut variable, &bytes)
}

pub(crate) fn erase_string(variable: &mut Value) {
    if let Value::String(bytes) = variable
        && let Some(bytes) = bytes.as_unique_bytes_mut()
    {
        bytes.zeroize();
    }
    *variable = Value::Null;
}

pub(crate) fn replace_state<const N: usize>(variable: &mut Value, bytes: [u8; N]) {
    let bytes = Zeroizing::new(bytes);
    erase_string(variable);
    *variable = Value::from(bytes.as_slice());
}
