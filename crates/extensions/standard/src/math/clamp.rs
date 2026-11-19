//! Keep a PHP value between two bounds using PHP's comparison rules.
//!
//! ```php
//! var_dump(clamp("9", 0, 5)); // int(5)
//! var_dump(clamp("3", 0, 5)); // string(1) "3"
//! ```

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Value, args,
};

pub(super) const ENTRY: FunctionEntry = FunctionEntry::asynchronous(
    "clamp",
    &[
        Param::new("value", ParamType::Mixed),
        Param::new("min", ParamType::Mixed),
        Param::new("max", ParamType::Mixed),
    ],
    clamp,
);

fn clamp(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let value = args::value(&arguments, 0);
        let minimum = args::value(&arguments, 1);
        let maximum = args::value(&arguments, 2);
        if matches!(minimum, Value::Float(number) if number.is_nan()) {
            return Err(NativeError::value_error(
                "clamp(): Argument #2 ($min) must not be NAN",
            ));
        }
        if matches!(maximum, Value::Float(number) if number.is_nan()) {
            return Err(NativeError::value_error(
                "clamp(): Argument #3 ($max) must not be NAN",
            ));
        }
        if context.compare(maximum, minimum).await? == -1 {
            return Err(NativeError::value_error(
                "clamp(): Argument #2 ($min) must be smaller than or equal to argument #3 ($max)",
            ));
        }
        let selected = if context.compare(maximum, value).await? == -1 {
            maximum
        } else if context.compare(value, minimum).await? == -1 {
            minimum
        } else {
            value
        };
        Ok(selected.try_deref()?.clone())
    })
}
