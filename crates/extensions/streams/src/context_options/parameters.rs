//! `stream_context_get_params($context)['notification']` returns its PHP callback.

use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, args};

use super::{Options, merge, read_options, update_options, writable_context};

pub(super) fn get_params(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let options = read_options(context, arguments, "stream_context_get_params", "context")?;
    let mut result = PhpArray::new();
    if let Some(notification) = options.notification {
        result.insert(ArrayKey::str("notification"), notification);
    }
    result.insert(ArrayKey::str("options"), Value::from(options.wrappers));
    Ok(Value::from(result))
}

pub(super) fn set_params(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let function = "stream_context_set_params";
    let target = writable_context(context, args::value(arguments, 0), function)?;
    if let Some(params) = args::array(arguments, 1) {
        // Resolve the callback with no borrow of the resource held.
        let mut options = update_options(&target, function, |data| Ok(data.clone()))?;
        let result = update_params(context, &mut options, params, function, "context");
        update_options(&target, function, |data| {
            *data = options;
            Ok(())
        })?;
        result?;
    }
    Ok(Value::Bool(true))
}

pub(super) fn update_params(
    context: &mut dyn Context,
    data: &mut Options,
    params: &PhpArray,
    function: &str,
    parameter: &str,
) -> Result<(), NativeError> {
    if let Some(notification) = params.get(&ArrayKey::str("notification")) {
        data.notification = None;
        context
            .validate_callback(function, 1, parameter, notification)
            .map_err(|error| match error {
                NativeError::Throw {
                    class: "TypeError",
                    message,
                    ..
                } => NativeError::type_error(message.replacen(
                    "must be a valid callback",
                    "must be an array with valid callbacks as values",
                    1,
                )),
                error => error,
            })?;
        data.notification = Some(notification.clone());
    }
    if let Some(input) = params.get(&ArrayKey::str("options")) {
        let value = input.deref();
        let Value::Array(array) = &*value else {
            return Err(NativeError::type_error("Invalid stream/context parameter"));
        };
        merge(&mut data.wrappers, array)?;
    }
    Ok(())
}
