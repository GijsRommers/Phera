//! `debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 2)` reads the two nearest callers.

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

const PROVIDE_OBJECT: i64 = 1;
const IGNORE_ARGS: i64 = 2;
const LIMIT: Param = Param::new("limit", ParamType::Int).with_default(ConstantValue::Int(0));

pub(crate) fn register(registry: &mut Registry) {
    registry.constant("DEBUG_BACKTRACE_PROVIDE_OBJECT", PROVIDE_OBJECT);
    registry.constant("DEBUG_BACKTRACE_IGNORE_ARGS", IGNORE_ARGS);
    registry.function(FunctionEntry::sync(
        "debug_backtrace",
        &[
            Param::new("options", ParamType::Int).with_default(ConstantValue::Int(PROVIDE_OBJECT)),
            LIMIT,
        ],
        backtrace,
    ));
    registry.function(FunctionEntry::sync(
        "debug_print_backtrace",
        &[
            Param::new("options", ParamType::Int).with_default(ConstantValue::Int(0)),
            LIMIT,
        ],
        print_backtrace,
    ));
}

fn backtrace(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let options = args::int(arguments, 0).unwrap_or(PROVIDE_OBJECT);
    let limit = args::int(arguments, 1).unwrap_or_default() as i32;
    Ok(context.backtrace(
        options & PROVIDE_OBJECT != 0,
        options & IGNORE_ARGS == 0,
        limit,
    ))
}

fn print_backtrace(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let options = args::int(arguments, 0).unwrap_or_default();
    let limit = args::int(arguments, 1).unwrap_or_default() as i32;
    let trace = context.backtrace(false, options & IGNORE_ARGS == 0, limit);
    context.print_backtrace(&trace);
    Ok(Value::Null)
}
