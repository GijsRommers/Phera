//! Captured closure chains are freed iteratively: `$next = fn() => $previous;` may be very deep.

use std::rc::Rc;

use phera_value::Value;

use crate::call_state::CallTarget;
use crate::closures::ClosureState;

impl Drop for ClosureState {
    /// Native payloads are opaque to the value crate's iterative array/object drop.
    fn drop(&mut self) {
        let mut pending = Vec::new();
        drain_state(self, &mut pending);
        while let Some(value) = pending.pop() {
            match value {
                Value::Reference(reference) => {
                    if let Ok(reference) = Rc::try_unwrap(reference) {
                        pending.push(reference.into_inner());
                    }
                }
                Value::Array(array) if Rc::strong_count(&array) == 1 => {
                    pending.extend(array.values().cloned());
                    // Pending owners prevent the array's own drop from freeing native payloads.
                }
                Value::Object(object) if Rc::strong_count(&object) == 1 => {
                    let native = object.native.borrow_mut().take();
                    if let Some(native) = native
                        && let Ok(mut state) = native.downcast::<ClosureState>()
                    {
                        drain_state(&mut state, &mut pending);
                    }
                    let properties = object.to_array();
                    pending.extend(properties.values().cloned());
                    object.update(|properties| *properties = Default::default());
                }
                _ => {}
            }
        }
    }
}

/// Empty a state before dropping it so the next payload's Drop has constant depth.
fn drain_state(state: &mut ClosureState, pending: &mut Vec<Value>) {
    pending.extend(state.captures.drain(..).flatten());
    if Rc::strong_count(&state.statics) == 1 {
        pending.extend(state.statics.borrow_mut().drain().map(|(_, value)| value));
    }
    if let Some(context) = &mut state.context
        && let Some(receiver) = context.receiver.take()
    {
        pending.push(Value::Object(receiver));
    }
    if let Some(callee) = Rc::get_mut(&mut state.callee)
        && let CallTarget::Method { context, .. } = &mut callee.target
        && let Some(receiver) = context.receiver.take()
    {
        pending.push(Value::Object(receiver));
    }
}
