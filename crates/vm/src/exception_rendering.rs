//! Render stored Throwable snapshots without consulting frames already unwound.

use crate::exceptions::throwable_root;
use phera_value::{ArrayKey, Object, PhpArray, Value};
use std::rc::Rc;

/// PHP prints previous exceptions oldest first, separated by Next blocks.
pub(crate) fn describe_chain(object: &Rc<Object>, string_limit: usize) -> String {
    let mut chain = Vec::new();
    let mut current = Some(object.clone());
    while let Some(object) = current {
        if chain.iter().any(|seen| Rc::ptr_eq(seen, &object)) {
            break;
        }
        current = match object.get(&Object::private_key(throwable_root(&object), "previous")) {
            Some(Value::Object(previous)) => Some(previous),
            _ => None,
        };
        chain.push(object);
    }
    chain
        .iter()
        .rev()
        .map(|object| describe(object, string_limit))
        .collect::<Vec<_>>()
        .join("\n\nNext ")
}

/// Argument-binding errors retain PHP's connective without changing getMessage().
fn describe(object: &Object, string_limit: usize) -> String {
    let message = protected_text(object, "message");
    let connective = if message.contains(", called in ") {
        "and defined in"
    } else {
        "in"
    };
    let separator = if message.is_empty() { "" } else { ": " };
    let trace = object
        .get(&Object::private_key(throwable_root(object), "trace"))
        .unwrap_or_default();
    format!(
        "{}{separator}{message} {connective} {}:{}\nStack trace:\n{}",
        object.class.name(),
        protected_text(object, "file"),
        protected_text(object, "line"),
        render_trace(&trace, string_limit)
    )
}

/// Trace formatting preserves named variadic arguments and PHP float spelling.
pub(crate) fn render_trace(trace: &Value, string_limit: usize) -> String {
    let Value::Array(frames) = trace else {
        return "#0 {main}".into();
    };
    let mut lines = Vec::new();
    for (index, frame) in frames.values().enumerate() {
        let Value::Array(frame) = frame else {
            continue;
        };
        let location = if frame.contains_key(&ArrayKey::str("file")) {
            format!("{}({}): ", field(frame, "file"), field(frame, "line"))
        } else {
            "[internal function]: ".into()
        };
        let arguments = match frame.get(&ArrayKey::str("args")) {
            Some(Value::Array(arguments)) => arguments
                .iter()
                .map(|(key, value)| {
                    let value = match &*value.deref() {
                        Value::Array(_) => "Array".into(),
                        Value::Object(object) => format!("Object({})", object.class.name()),
                        _ => crate::error_values::describe_with_limit(value, string_limit),
                    };
                    match key {
                        ArrayKey::String(name) => format!("{}: {value}", name.to_string_lossy()),
                        _ => value,
                    }
                })
                .collect::<Vec<_>>()
                .join(", "),
            _ => String::new(),
        };
        lines.push(format!(
            "#{index} {location}{}{}{}({arguments})",
            field(frame, "class"),
            field(frame, "type"),
            field(frame, "function")
        ));
    }
    lines.push(format!("#{} {{main}}", frames.len()));
    lines.join("\n")
}

/// PHP's diagnostic properties are scalar values and need no magic conversion.
fn protected_text(object: &Object, name: &str) -> String {
    text(object.get(&Object::protected_key(name)).as_ref())
}

/// Missing optional frame fields contribute no text.
fn field(frame: &PhpArray, name: &str) -> String {
    text(frame.get(&ArrayKey::str(name)))
}

/// Scalar formatting here cannot run user code while handling an uncaught error.
fn text(value: Option<&Value>) -> String {
    value
        .and_then(|value| phera_value::to_php_string(value, &mut phera_value::NoDiagnostics).ok())
        .map(|value| value.to_string_lossy())
        .unwrap_or_default()
}
