//! Lexical closures use identity; callable wrappers compare targets.
//! For example, `strlen(...) == strlen(...)` is true.

use std::rc::Rc;

/// PHP compares fake closure metadata without comparing captures or mutable static variables.
pub(crate) fn same_callable(left: &phera_value::Object, right: &phera_value::Object) -> bool {
    let left = left.native.borrow();
    let right = right.native.borrow();
    let left = left
        .as_ref()
        .and_then(|value| value.downcast_ref::<crate::closures::ClosureState>());
    let right = right
        .as_ref()
        .and_then(|value| value.downcast_ref::<crate::closures::ClosureState>());
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    if left.origin != crate::closures::ClosureOrigin::Callable
        || right.origin != crate::closures::ClosureOrigin::Callable
        || !left.callee.name.eq_ignore_ascii_case(&right.callee.name)
        || implementation(&left.callee.target) != implementation(&right.callee.target)
    {
        return false;
    }
    match (&left.context, &right.context) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            let same_receiver = match (&left.receiver, &right.receiver) {
                (None, None) => true,
                (Some(left), Some(right)) => Rc::ptr_eq(left, right),
                _ => false,
            };
            same_receiver
                && Rc::ptr_eq(&left.scope, &right.scope)
                && Rc::ptr_eq(&left.called, &right.called)
        }
        _ => false,
    }
}

/// A user function and an internal function cannot compare as the same implementation.
#[derive(PartialEq, Eq)]
enum Implementation {
    Php,
    Internal,
}

fn implementation(target: &crate::calls::call_state::CallTarget) -> Option<Implementation> {
    use crate::calls::call_state::CallTarget;
    match target {
        CallTarget::User { .. } => Some(Implementation::Php),
        CallTarget::Native(_) | CallTarget::Intrinsic(_) => Some(Implementation::Internal),
        CallTarget::Method { method, .. } => match method.body {
            crate::classes::MethodBody::User(_) => Some(Implementation::Php),
            crate::classes::MethodBody::Native(_)
            | crate::classes::MethodBody::Closure(_)
            | crate::classes::MethodBody::Enum(_) => Some(Implementation::Internal),
        },
        CallTarget::Closure(_) => None,
    }
}
