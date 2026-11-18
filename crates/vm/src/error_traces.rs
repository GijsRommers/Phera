//! Render uncaught PHP errors from heap frames, including failed parameter binding.

use crate::{Vm, calls::frame::FrameView};
use phera_value::Value;

/// Native calls have no bytecode frame, so capture only their cold error path.
#[derive(Clone)]
pub(crate) struct NativeTrace {
    pub(crate) name: String,
    pub(crate) arguments: Value,
}

impl NativeTrace {
    /// Render before native argument storage is released, without retaining PHP values.
    pub(crate) fn new(name: &str, arguments: &[Value]) -> Self {
        Self {
            name: name.into(),
            arguments: {
                let mut array = phera_value::PhpArray::new();
                for value in arguments {
                    let _ = array.push(value.deref().clone());
                }
                Value::Array(std::rc::Rc::new(array))
            },
        }
    }
}

/// Exception traces label arrays and objects without recursively dumping their contents.
fn argument(value: &Value) -> String {
    match &*value.deref() {
        Value::Array(_) => "Array".into(),
        Value::Object(object) => format!("Object({})", object.class.name()),
        _ => crate::error_values::describe(value),
    }
}

/// Read user parameter slots after coercion, as PHP's stack traces do.
fn frame_arguments(vm: &Vm, frame: FrameView<'_>) -> String {
    let mut arguments = Vec::new();
    for (index, parameter) in frame.function().parameters.iter().enumerate() {
        let Some(Some(value)) = frame.slots(&vm.registers).get(index) else {
            continue;
        };
        if parameter.variadic {
            if let Value::Array(values) = &*value.deref() {
                for (key, value) in values.iter() {
                    let text = argument(value);
                    arguments.push(match key {
                        phera_value::ArrayKey::String(name) => {
                            format!("{}: {text}", name.to_string_lossy())
                        }
                        _ => text,
                    });
                }
            }
        } else {
            arguments.push(argument(value));
        }
    }
    arguments.join(", ")
}

impl Vm {
    /// Binding failures point at the declaration and keep the caller as trace frame zero.
    pub(crate) fn error_trace(
        &mut self,
        class: &str,
        message: &str,
        file: &str,
        line: usize,
    ) -> String {
        let binding_failure = self
            .current()
            .is_some_and(|frame| frame.ip() == 0 && frame.function_id().is_some());
        let connective = if binding_failure {
            "and defined in"
        } else {
            "in"
        };
        let mut text = format!(
            "\nFatal error: Uncaught {class}: {message} {connective} {file}:{line}\nStack trace:\n"
        );
        let mut number = 0;
        if let Some(native) = self.failed_native.take() {
            text.push_str(&format!(
                "#0 {file}({line}): {}({})\n",
                native.name,
                match &native.arguments {
                    Value::Array(values) =>
                        values.values().map(argument).collect::<Vec<_>>().join(", "),
                    _ => String::new(),
                }
            ));
            number += 1;
        }
        for (parent, child) in self.frames().rev().skip(1).zip(self.frames().rev()) {
            text.push_str(&format!(
                "#{number} {}({}): {}({})\n",
                parent.unit().file,
                parent.line(),
                if child
                    .method()
                    .is_some_and(|context| context.receiver.is_some())
                {
                    child.function().name.replacen("::", "->", 1)
                } else {
                    child.function().name.clone()
                },
                frame_arguments(self, child)
            ));
            number += 1;
        }
        text.push_str(&format!(
            "#{number} {{main}}\n  thrown in {file} on line {line}\n"
        ));
        text
    }
}

/// Declaration incompatibilities are fatal diagnostics rather than catchable runtime Errors.
pub(crate) struct DeclarationFailure {
    pub(crate) message: String,
    pub(crate) file: String,
    pub(crate) line: usize,
}
