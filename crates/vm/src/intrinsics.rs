//! Intrinsics are resolved once per call symbol, before PHP arguments run.

/// VM services that need access to the caller scope or request state.
#[derive(Clone, Copy)]
pub(crate) enum Intrinsic {
    /// Callback mapping needs the request callable resolver.
    ArrayMap,
    Output(crate::output_buffer::OutputFunction),
    Clone,
    /// Request-level PHP handlers use VM callable resolution.
    Handler(crate::error_handlers::HandlerFunction),
    /// Path basename is needed to inspect portable Throwable locations.
    Basename,
    /// Linked class introspection needs VM metadata.
    Class(crate::class_intrinsics::ClassFunction),
    /// SPL loaders belong to the current request.
    SplAutoload(crate::spl_autoload::SplAutoload),
    /// Runtime extension and function listings.
    Runtime(crate::function_introspection::RuntimeFunction),
    GetIncludePath,
    SetIncludePath,
    /// PHP `is_callable()` runtime service.
    IsCallable,
    /// PHP `call_user_func()` runtime service.
    CallUserFunc,
    /// PHP `call_user_func_array()` runtime service.
    CallUserFuncArray,
    /// PHP `get_defined_vars()` runtime service.
    GetDefinedVars,
    /// PHP `compact()` runtime service.
    Compact,
    /// PHP `extract()` runtime service.
    Extract,
    /// PHP `define()` runtime service.
    Define,
    /// PHP `defined()` runtime service.
    Defined,
    /// PHP `constant()` runtime service.
    Constant,
    /// PHP `function_exists()` runtime service.
    FunctionExists,
    /// PHP `func_num_args()` runtime service.
    FuncNumArgs,
    /// PHP `func_get_args()` runtime service.
    FuncGetArgs,
    /// PHP `func_get_arg()` runtime service.
    FuncGetArg,
    /// PHP `error_reporting()` runtime service.
    ErrorReporting,
    /// PHP `ini_set()` runtime service.
    IniSet,
    /// PHP `ini_get()` runtime service.
    IniGet,
}

impl Intrinsic {
    /// Resolve the case-insensitive PHP function name at the call boundary.
    pub(crate) fn resolve(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "set_exception_handler" => {
                Self::Handler(crate::error_handlers::HandlerFunction::SetException)
            }
            "restore_exception_handler" => {
                Self::Handler(crate::error_handlers::HandlerFunction::RestoreException)
            }
            "set_error_handler" => Self::Handler(crate::error_handlers::HandlerFunction::SetError),
            "restore_error_handler" => {
                Self::Handler(crate::error_handlers::HandlerFunction::RestoreError)
            }
            "get_exception_handler" => {
                Self::Handler(crate::error_handlers::HandlerFunction::GetException)
            }
            "get_error_handler" => Self::Handler(crate::error_handlers::HandlerFunction::GetError),
            "trigger_error" | "user_error" => {
                Self::Handler(crate::error_handlers::HandlerFunction::TriggerError)
            }
            "clone" => Self::Clone,
            "array_map" => Self::ArrayMap,
            "basename" => Self::Basename,
            "is_callable" => Self::IsCallable,
            "call_user_func" => Self::CallUserFunc,
            "call_user_func_array" => Self::CallUserFuncArray,
            "get_defined_vars" => Self::GetDefinedVars,
            "compact" => Self::Compact,
            "extract" => Self::Extract,
            "define" => Self::Define,
            "defined" => Self::Defined,
            "constant" => Self::Constant,
            "function_exists" => Self::FunctionExists,
            "func_num_args" => Self::FuncNumArgs,
            "func_get_args" => Self::FuncGetArgs,
            "func_get_arg" => Self::FuncGetArg,
            "error_reporting" => Self::ErrorReporting,
            "ini_set" => Self::IniSet,
            "ini_get" => Self::IniGet,
            "get_include_path" => Self::GetIncludePath,
            "set_include_path" => Self::SetIncludePath,
            name => {
                if let Some(function) = crate::class_intrinsics::ClassFunction::resolve(name) {
                    Self::Class(function)
                } else if let Some(function) = crate::spl_autoload::SplAutoload::resolve(name) {
                    Self::SplAutoload(function)
                } else if let Some(function) = crate::output_buffer::OutputFunction::resolve(name) {
                    Self::Output(function)
                } else {
                    Self::Runtime(crate::function_introspection::RuntimeFunction::resolve(
                        name,
                    )?)
                }
            }
        })
    }
}

impl Intrinsic {
    /// Names are resolved once so handler intrinsics support PHP named arguments.
    pub(crate) fn parameter_names(self) -> &'static [&'static str] {
        use crate::error_handlers::HandlerFunction;
        match self {
            Self::Handler(HandlerFunction::SetException) => &["callback"],
            Self::Handler(HandlerFunction::SetError) => &["callback", "error_levels"],
            Self::Handler(HandlerFunction::TriggerError) => &["message", "error_level"],
            Self::ErrorReporting => &["error_level"],
            Self::FuncGetArg => &["position"],
            Self::IsCallable => &["value", "syntax_only", "callable_name"],
            Self::CallUserFunc => &["callback", "args"],
            Self::CallUserFuncArray => &["callback", "args"],
            Self::Clone => &["object", "withProperties"],
            Self::ArrayMap => &["callback", "array", "arrays"],
            Self::Basename => &["path", "suffix"],
            Self::FunctionExists => &["function"],
            Self::Defined => &["constant_name"],
            Self::Constant => &["name"],
            Self::SetIncludePath => &["include_path"],
            Self::IniGet => &["option"],
            Self::IniSet => &["option", "value"],
            _ => &[],
        }
    }
}

impl Intrinsic {
    pub(crate) fn parameters(self) -> Option<&'static [phera_extension_api::Param]> {
        match self {
            Self::Class(function) => Some(function.parameters()),
            Self::SplAutoload(function) => Some(function.parameters()),
            Self::Runtime(function) => Some(function.parameters()),
            Self::Output(function) => Some(function.parameters()),
            _ => None,
        }
    }

    pub(crate) fn names() -> impl Iterator<Item = &'static str> {
        [
            "array_map",
            "basename",
            "is_callable",
            "call_user_func",
            "call_user_func_array",
            "get_defined_vars",
            "compact",
            "extract",
            "define",
            "defined",
            "constant",
            "function_exists",
            "func_num_args",
            "func_get_args",
            "func_get_arg",
            "error_reporting",
            "ini_set",
            "ini_get",
            "get_include_path",
            "set_include_path",
            "set_error_handler",
            "restore_error_handler",
            "set_exception_handler",
            "restore_exception_handler",
            "get_error_handler",
            "get_exception_handler",
            "trigger_error",
            "user_error",
        ]
        .into_iter()
        .chain(
            crate::class_intrinsics::ClassFunction::ALL
                .iter()
                .map(|function| function.name()),
        )
        .chain(
            crate::spl_autoload::SplAutoload::ALL
                .iter()
                .map(|function| function.name()),
        )
        .chain(
            crate::output_buffer::OutputFunction::ALL
                .iter()
                .map(|function| function.name()),
        )
        .chain(
            crate::function_introspection::RuntimeFunction::ALL
                .iter()
                .map(|function| function.name()),
        )
    }

    /// Named argument gaps differ from an explicitly passed null.
    pub(crate) fn named_default(self, position: usize) -> phera_value::Value {
        use phera_value::Value;
        match (self, position) {
            (Self::Output(crate::output_buffer::OutputFunction::Start), 1) => Value::Int(0),
            (Self::IsCallable, 1) => Value::Bool(false),
            (Self::SplAutoload(crate::spl_autoload::SplAutoload::Register), 1) => Value::Bool(true),
            _ => Value::Null,
        }
    }
}
