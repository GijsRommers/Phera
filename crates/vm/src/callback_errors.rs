//! Callback parameters explain resolution failures: `call_user_func('missing')` raises TypeError.

use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// The resolver's Error text and callback APIs' reason text differ in PHP itself.
    pub(crate) fn callback_reason(&self, callable: &Value, error: NativeError) -> String {
        let Value::String(name) = &*callable.deref() else {
            return callback_detail(error);
        };
        if !name.as_bytes().contains(&b':') {
            return format!(
                "function \"{}\" not found or invalid function name",
                name.to_string_lossy()
            );
        }
        callback_detail(error)
    }

    pub(crate) fn invalid_callback(
        &self,
        function: &str,
        number: usize,
        parameter: &str,
        callable: &Value,
        error: NativeError,
        nullable: bool,
    ) -> NativeError {
        if crate::unsupported_items::is_unsupported(&error) {
            return error;
        }
        let detail = self.callback_reason(callable, error);
        let null = if nullable { " or null" } else { "" };
        NativeError::type_error(format!(
            "{function}(): Argument #{number} (${parameter}) {requirement}{null}, {detail}",
            requirement = "must be a valid callback"
        ))
    }
}

/// Translate existing engine failures at the callback boundary without changing invocation errors.
fn callback_detail(error: NativeError) -> String {
    let NativeError::Throw { message, .. } = error else {
        return "no array or string given".into();
    };
    if let Some(name) = message.strip_prefix("Call to undefined method ") {
        let name = name.strip_suffix("()").unwrap_or(name);
        if let Some((class, method)) = name.split_once("::") {
            return format!("class {class} does not have a method \"{method}\"");
        }
    }
    for visibility in ["private", "protected"] {
        let prefix = format!("Call to {visibility} method ");
        if let Some(name) = message.strip_prefix(&prefix) {
            let name = name.split_once("() from ").map_or(name, |(name, _)| name);
            return format!("cannot access {visibility} method {name}()");
        }
    }
    if let Some(class) = message.strip_prefix("Class ") {
        return format!("class {class}");
    }
    if let Some(method) = message.strip_prefix("Non-static method ") {
        return format!("non-static method {method}");
    }
    match message.as_str() {
        "Array callback must have exactly two elements" => {
            "array callback must have exactly two members".into()
        }
        "Array callback has to contain indices 0 and 1" => {
            "array callback has to contain indices 0 and 1".into()
        }
        "First array member is not a valid class name or object" => {
            "first array member is not a valid class name or object".into()
        }
        "Second array member is not a valid method" => {
            "second array member is not a valid method".into()
        }
        _ if message.starts_with("Object of type ") => "no array or string given".into(),
        _ if message.starts_with("Value of type ") => "no array or string given".into(),
        _ => message,
    }
}
