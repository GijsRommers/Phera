//! Intrinsics are resolved once per call symbol, before PHP arguments run.

/// VM services that need access to the caller scope or request state.
#[derive(Clone, Copy)]
pub(crate) enum Intrinsic {
    /// Linked class introspection needs VM metadata.
    Class(crate::class_intrinsics::ClassFunction),
    /// Visibility-aware instance properties.
    GetObjectVars,
    /// Visibility-aware default instance and static properties.
    GetClassVars,
    /// Declaration and dynamic-property existence, independent of visibility.
    PropertyExists,
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
            "get_object_vars" => Self::GetObjectVars,
            "get_class_vars" => Self::GetClassVars,
            "property_exists" => Self::PropertyExists,
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
            _ => Self::Class(crate::class_intrinsics::ClassFunction::resolve(
                &name.to_ascii_lowercase(),
            )?),
        })
    }
}
