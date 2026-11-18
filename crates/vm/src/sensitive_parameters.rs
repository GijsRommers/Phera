//! `#[SensitiveParameter]` replaces values with wrappers in exception traces.

use crate::{Vm, calls::frame::FrameView};
use phera_value::Value;

pub(crate) fn is_sensitive(frame: FrameView<'_>, position: usize) -> bool {
    frame
        .function_id()
        .and_then(|index| frame.unit().function_metadata.get(index))
        .and_then(|function| function.parameters.get(position))
        .is_some_and(|parameter| {
            crate::attribute_diagnostics::attribute(&parameter.attributes, "SensitiveParameter")
                .is_some()
        })
}

impl Vm {
    pub(crate) fn sensitive_trace_value(&mut self, value: Value) -> Value {
        let Some(class) = self.classes.get("SensitiveParameterValue") else {
            // A VM embedded without Core attribute classes still must redact the trace.
            return Value::Null;
        };
        let wrapped = self.classes.allocate(class);
        *wrapped.native.borrow_mut() = Some(Box::new(value));
        Value::Object(wrapped)
    }
}
