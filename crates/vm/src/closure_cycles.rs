//! Trace what a closure owns, as in `$object->callback = fn () => $object;`.

use std::{any::Any, rc::Rc};

use phera_value::{Object, Value};

use crate::{calls::call_state::CallTarget, closures::ClosureState};

pub(crate) fn trace(payload: &dyn Any, visit: &mut dyn FnMut(&Value)) -> bool {
    let Some(state) = payload.downcast_ref::<ClosureState>() else {
        return false;
    };
    let statics = if let Some(statics) = &state.statics
        && Rc::strong_count(statics) == 1
    {
        let Ok(values) = statics.try_borrow() else {
            return false;
        };
        Some(values)
    } else {
        // An active frame also owns the static cells. Those handles are
        // outside this closure's edges until that frame lets go.
        None
    };
    state.captures.iter().flatten().for_each(&mut *visit);
    if let Some(statics) = &statics {
        statics.values().for_each(&mut *visit);
    }
    let mut receiver = |object: &Rc<Object>| visit(&Value::Object(object.clone()));
    if let Some(object) = state
        .context
        .as_ref()
        .and_then(|context| context.receiver.as_ref())
    {
        receiver(object);
    }
    match &state.callee.target {
        CallTarget::Method { context, .. } => {
            if let Some(object) = &context.receiver {
                receiver(object);
            }
        }
        CallTarget::Closure(object) => receiver(object),
        CallTarget::Magic(call) if Rc::strong_count(call) == 1 => {
            if let Some(object) = &call.context.receiver {
                receiver(object);
            }
        }
        _ => {}
    }
    true
}
