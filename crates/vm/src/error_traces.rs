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
            arguments: Self::snapshot_arguments(arguments),
        }
    }

    /// A PHP trace distinguishes `$reflector->getName()` from `Class::method()`.
    #[cold]
    pub(crate) fn method(method: &crate::classes::Method, arguments: &[Value]) -> Self {
        let operator = if method.is_static { "::" } else { "->" };
        Self {
            name: format!("{}{operator}{}", method.declaring, method.name),
            arguments: Self::snapshot_arguments(arguments),
        }
    }

    fn snapshot_arguments(arguments: &[Value]) -> Value {
        let mut array = phera_value::PhpArray::new();
        for value in arguments {
            let _ = array.push(value.deref().clone());
        }
        Value::from(array)
    }
}

/// Exception traces label arrays and objects without recursively dumping their contents.
fn argument(value: &Value) -> String {
    crate::error_values::trace_argument(value, 15)
}

/// Read user parameter slots after coercion, as PHP's stack traces do.
fn frame_arguments(vm: &Vm, frame: FrameView<'_>) -> String {
    let mut arguments = Vec::new();
    for (index, parameter) in frame.function().parameters.iter().enumerate() {
        let Some(Some(value)) = frame.slots(&vm.registers).get(index) else {
            continue;
        };
        let sensitive = crate::sensitive_parameters::is_sensitive(frame, index);
        let describe = |value: &Value| {
            if sensitive {
                "Object(SensitiveParameterValue)".into()
            } else {
                argument(value)
            }
        };
        if parameter.variadic {
            if let Value::Array(values) = &*value.deref() {
                for (key, value) in values.iter() {
                    let text = describe(value);
                    arguments.push(match key {
                        phera_value::ArrayKey::String(name) => {
                            format!("{}: {text}", name.to_string_lossy())
                        }
                        _ => text,
                    });
                }
            }
        } else {
            arguments.push(describe(value));
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
                if child.this().is_some() {
                    self.code
                        .function_name(child.frame.function)
                        .replacen("::", "->", 1)
                } else {
                    self.code.function_name(child.frame.function).to_owned()
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

/// Native operations that enter PHP record the caller without allocating a PHP frame.
pub(crate) struct NativeCallbackSite {
    pub(crate) boundary: usize,
    pub(crate) native: NativeCaller,
}

/// `array_map($callback, $values)` already owns its arguments for the duration of the call.
pub(crate) enum NativeCaller {
    Saved(NativeTrace),
    ArrayMap(Vec<Value>),
    Method {
        name: &'static str,
        object: std::rc::Rc<phera_value::Object>,
        arguments: Vec<Value>,
        declaration: Option<std::rc::Rc<phera_extension_api::reflection::AttributeSite>>,
    },
    Internal(Option<crate::diagnostics::SourceLocation>),
}

impl NativeCaller {
    pub(crate) fn name(&self) -> Option<&str> {
        match self {
            Self::Saved(trace) => Some(&trace.name),
            Self::Method { name, .. } => Some(name),
            Self::ArrayMap(_) => Some("array_map"),
            Self::Internal(_) => None,
        }
    }

    pub(crate) fn arguments(&self) -> Value {
        match self {
            Self::Saved(trace) => trace.arguments.clone(),
            Self::ArrayMap(arguments) | Self::Method { arguments, .. } => {
                NativeTrace::snapshot_arguments(arguments)
            }
            Self::Internal(_) => Value::Null,
        }
    }

    pub(crate) fn object(&self) -> Option<&std::rc::Rc<phera_value::Object>> {
        match self {
            Self::Method { object, .. } => Some(object),
            _ => None,
        }
    }

    pub(crate) fn child_location(&self) -> Option<(&str, usize)> {
        match self {
            Self::Method {
                declaration: Some(site),
                ..
            } => Some((&site.file, site.line)),
            Self::Internal(Some(location)) => Some((&location.file, location.line)),
            _ => None,
        }
    }
}

impl Vm {
    pub(crate) fn push_native_callback(&mut self, native: NativeCaller) {
        self.native_callback_sites.push(NativeCallbackSite {
            boundary: self.call_stack.depth(),
            native,
        });
    }
}
