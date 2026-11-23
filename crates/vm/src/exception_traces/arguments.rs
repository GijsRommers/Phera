//! `function f($x) { $x++; return debug_backtrace(); }` shows the new value of $x.

use phera_value::{ArrayKey, PhpArray, Value};

use crate::{
    Vm,
    calls::frame::FrameView,
    classes::ClassTable,
    code_table::{CodeTable, FunctionIndex},
};

impl Vm {
    /// PHP snapshots live parameters and the original variadic list.
    pub(super) fn trace_arguments(&mut self, depth: usize) -> Value {
        let Some(frame) = self.frame_at(depth) else {
            return Value::from(PhpArray::new());
        };
        let snapshot = self.frame_trace_arguments(frame);
        materialize_trace_arguments(&mut self.classes, snapshot)
    }

    pub(crate) fn frame_trace_arguments(&self, frame: FrameView<'_>) -> TraceArguments {
        let parameter_count =
            if frame.function().argument_layout == phera_bytecode::ArgumentLayout::Variadic {
                frame.argument_count()
            } else {
                frame.supplied_arguments()
            };
        snapshot_arguments(
            &self.code,
            frame.frame.function,
            parameter_count,
            frame.slots(&self.registers),
            self.registers
                .window(frame.base() + frame.count(), frame.extras.arguments.extra),
            frame.extras.variadic.as_deref(),
        )
    }
}

pub(crate) type TraceArguments = Vec<(Option<ArrayKey>, Value, bool)>;

/// The argument window can belong to a live or a suspended PHP activation.
pub(crate) fn snapshot_arguments(
    code: &CodeTable,
    function: FunctionIndex,
    count: usize,
    slots: &[Option<Value>],
    extra: &[Option<Value>],
    original_variadic: Option<&PhpArray>,
) -> TraceArguments {
    let params = &code.function(function).parameters;
    let variadic = params.last().is_some_and(|parameter| parameter.variadic);
    let fixed = params.len() - usize::from(variadic);
    let mut snapshot = Vec::new();
    for (index, slot) in slots.iter().take(fixed.min(count)).enumerate() {
        let value = slot
            .as_ref()
            .map_or(Value::Null, |value| value.deref().clone());
        let sensitive = crate::sensitive_parameters::is_sensitive_at(code, function, index);
        snapshot.push((None, value, sensitive));
    }
    if let Some(items) = original_variadic {
        let sensitive = crate::sensitive_parameters::is_sensitive_at(code, function, fixed);
        for (key, value) in items.iter() {
            let name = match key {
                ArrayKey::Interned(_) | ArrayKey::String(_) => Some(key),
                _ => None,
            };
            snapshot.push((name, value.deref().clone(), sensitive));
        }
    } else if !variadic {
        for slot in extra {
            let value = slot
                .as_ref()
                .map_or(Value::Null, |value| value.deref().clone());
            snapshot.push((None, value, false));
        }
    }
    snapshot
}

pub(crate) fn materialize_trace_arguments(
    classes: &mut ClassTable,
    snapshot: TraceArguments,
) -> Value {
    let mut arguments = PhpArray::new();
    for (key, value, sensitive) in snapshot {
        let value = if sensitive {
            crate::sensitive_parameters::wrap_value(classes, value)
        } else {
            value
        };
        if let Some(key) = key {
            arguments.insert(key, value);
        } else {
            let _ = arguments.push(value);
        }
    }
    Value::from(arguments)
}
