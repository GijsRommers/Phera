//! `@trigger_error('warning'); echo error_get_last()['message'];` prints "warning".

use phera_extension_api::{Context, FunctionEntry, NativeResult, Registry, Value};

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("error_get_last", &[], get));
    registry.function(FunctionEntry::sync("error_clear_last", &[], clear));
    registry.describe_function("error_get_last", "?array", None);
    registry.describe_function("error_clear_last", "void", None);
}

fn get(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(context.last_error())
}

fn clear(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    context.clear_last_error();
    Ok(Value::Null)
}
