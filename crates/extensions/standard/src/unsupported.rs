//! `dl($extension)` and other standard functions outside Phera's runtime scope.

use phera_extension_api::{
    ConstantValue, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry,
};

pub(super) fn register(registry: &mut Registry) {
    for entry in [
        FunctionEntry::sync(
            "get_browser",
            &[
                Param::new("user_agent", ParamType::String)
                    .nullable()
                    .with_default(ConstantValue::Null),
                Param::new("return_array", ParamType::Bool)
                    .with_default(ConstantValue::Bool(false)),
            ],
            |_, _| unsupported("get_browser", "Phera has no browser capability database"),
        ),
        FunctionEntry::sync(
            "register_tick_function",
            &[
                Param::new("callback", ParamType::Callable),
                Param::new("args", ParamType::Mixed).variadic(),
            ],
            |_, _| {
                unsupported(
                    "register_tick_function",
                    "Phera does not dispatch PHP ticks",
                )
            },
        ),
        FunctionEntry::sync(
            "dl",
            &[Param::new("extension_filename", ParamType::String)],
            |_, _| unsupported("dl", "Phera uses extensions compiled into the runtime"),
        ),
        FunctionEntry::sync(
            "output_add_rewrite_var",
            &[
                Param::new("name", ParamType::String),
                Param::new("value", ParamType::String),
            ],
            |_, _| {
                unsupported(
                    "output_add_rewrite_var",
                    "Phera does not rewrite outgoing HTML links",
                )
            },
        ),
    ] {
        registry.function(entry);
    }
}

fn unsupported(function: &str, reason: &str) -> NativeResult {
    Err(NativeError::throw(
        "Phera\\NotSupportedException",
        format!("{function}() is not supported: {reason}"),
    ))
}
