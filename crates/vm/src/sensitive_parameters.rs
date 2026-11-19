//! `#[SensitiveParameter]` replaces values with wrappers in exception traces.

use crate::calls::frame::FrameView;
use phera_value::{Object, Value};

pub(crate) fn is_sensitive(frame: FrameView<'_>, position: usize) -> bool {
    is_sensitive_at(frame.code, frame.frame.function, position)
}

pub(crate) fn is_sensitive_at(
    code: &crate::code_table::CodeTable,
    function: crate::code_table::FunctionIndex,
    position: usize,
) -> bool {
    code.local(function)
        .and_then(|index| code.unit(function).function_metadata.get(index))
        .and_then(|function| function.parameters.get(position))
        .is_some_and(|parameter| {
            crate::attribute_diagnostics::attribute(&parameter.attributes, "SensitiveParameter")
                .is_some()
        })
}

pub(crate) fn wrap_value(classes: &mut crate::classes::ClassTable, value: Value) -> Value {
    let Some(class) = classes.get("SensitiveParameterValue") else {
        // A VM embedded without Core attribute classes still must redact the trace.
        return Value::Null;
    };
    let wrapped = classes.allocate(class);
    wrapped.set(
        Object::private_key("SensitiveParameterValue", "value"),
        value,
    );
    Value::Object(wrapped)
}
