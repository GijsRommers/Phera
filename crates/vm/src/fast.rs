//! Execute scalar instructions while retaining the current code and register window.
//!
//! A local Arc owns the borrowed instruction slice independently of the VM. The
//! frame IP is published before diagnostics, calls, returns, or suspension.

use phera_bytecode::{IncDec, Instruction, Reg};
use phera_value::{Value, to_bool};

use crate::operators::fast_binary;

/// Argument introspection requires assignments to update the saved argument view.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgumentTracking {
    Observed,
    Unobserved,
}

/// A fast instruction either completes or delegates PHP coercion to the slow handler.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScalarStep {
    Complete,
    Fallback,
}

/// The destination and computed scalar travel together until the final slot write.
struct ScalarWrite {
    destination: Reg,
    value: Value,
}

impl ScalarWrite {
    #[inline]
    fn new(destination: Reg, value: Value) -> Self {
        Self { destination, value }
    }
}

/// Scalar cases avoid repeated frame lookup; coercions and aliases use handlers.
#[inline]
pub(crate) fn scalar_step(
    instruction: Instruction,
    slots: &mut [Option<Value>],
    ip: &mut usize,
    tracking: ArgumentTracking,
) -> ScalarStep {
    let write = match instruction {
        Instruction::Nop => return ScalarStep::Complete,
        Instruction::LoadInt {
            dst: destination,
            value,
        } => ScalarWrite::new(destination, Value::Int(value as i64)),
        Instruction::LoadNull { dst: destination } => ScalarWrite::new(destination, Value::Null),
        Instruction::LoadBool {
            dst: destination,
            value,
        } => ScalarWrite::new(destination, Value::Bool(value)),
        Instruction::Move {
            dst: destination,
            src: source,
        } => {
            let Some(Some(value)) = slots.get(source.index()) else {
                return ScalarStep::Fallback;
            };
            ScalarWrite::new(destination, value.deref().clone())
        }
        Instruction::AppendLocal { local, src: source } => {
            let Some(Some(value)) = slots.get(source.index()) else {
                return ScalarStep::Fallback;
            };
            let value = value.deref().clone();
            let Some(Some(Value::Array(array))) = slots.get_mut(local.index()) else {
                return ScalarStep::Fallback;
            };
            return if std::rc::Rc::make_mut(array).push(value).is_ok() {
                ScalarStep::Complete
            } else {
                ScalarStep::Fallback
            };
        }
        Instruction::BinaryIntBranch {
            op: operator,
            lhs: left,
            rhs: right,
            target,
        } => {
            let Some(Some(left)) = slots.get(left.index()) else {
                return ScalarStep::Fallback;
            };
            let Some(value) = fast_binary(operator, left, &Value::Int(right as i64)) else {
                return ScalarStep::Fallback;
            };
            if !to_bool(&value) {
                *ip = target.index()
            }
            return ScalarStep::Complete;
        }
        Instruction::BinaryInt {
            op: operator,
            dst: destination,
            lhs: left,
            rhs: right,
        } => {
            let Some(Some(left)) = slots.get(left.index()) else {
                return ScalarStep::Fallback;
            };
            let Some(value) = fast_binary(operator, left, &Value::Int(right as i64)) else {
                return ScalarStep::Fallback;
            };
            ScalarWrite::new(destination, value)
        }
        Instruction::Binary {
            op: operator,
            dst: destination,
            lhs: left,
            rhs: right,
        } => {
            let Some(Some(left)) = slots.get(left.index()) else {
                return ScalarStep::Fallback;
            };
            let Some(Some(right)) = slots.get(right.index()) else {
                return ScalarStep::Fallback;
            };
            let Some(value) = fast_binary(operator, left, right) else {
                return ScalarStep::Fallback;
            };
            ScalarWrite::new(destination, value)
        }
        Instruction::Assign { local, src: source } if tracking == ArgumentTracking::Unobserved => {
            if matches!(slots.get(local.index()), Some(Some(Value::Reference(_)))) {
                return ScalarStep::Fallback;
            }
            let Some(Some(value)) = slots.get(source.index()) else {
                return ScalarStep::Fallback;
            };
            ScalarWrite::new(local, value.deref().clone())
        }
        Instruction::AssignOp {
            op: operator,
            local,
            src: source,
        } => {
            let Some(Some(left)) = slots.get(local.index()) else {
                return ScalarStep::Fallback;
            };
            let Some(Some(right)) = slots.get(source.index()) else {
                return ScalarStep::Fallback;
            };
            let Some(value) = fast_binary(operator, left, right) else {
                return ScalarStep::Fallback;
            };
            ScalarWrite::new(local, value)
        }
        Instruction::IncDecLocal {
            mode,
            dst: destination,
            local,
        } => {
            let Some(Some(Value::Int(integer))) = slots.get(local.index()) else {
                return ScalarStep::Fallback;
            };
            let old = *integer;
            let increment = matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement);
            let Some(new) = old.checked_add(if increment { 1 } else { -1 }) else {
                return ScalarStep::Fallback;
            };
            let Some(slot) = slots.get_mut(local.index()) else {
                return ScalarStep::Fallback;
            };
            *slot = Some(Value::Int(new));
            ScalarWrite::new(
                destination,
                Value::Int(
                    if matches!(mode, IncDec::PostIncrement | IncDec::PostDecrement) {
                        old
                    } else {
                        new
                    },
                ),
            )
        }
        Instruction::Jump { target } => {
            *ip = target.index();
            return ScalarStep::Complete;
        }
        Instruction::JumpIf { condition, target }
        | Instruction::JumpIfNot { condition, target } => {
            let Some(Some(value)) = slots.get(condition.index()) else {
                return ScalarStep::Fallback;
            };
            if to_bool(value) == matches!(instruction, Instruction::JumpIf { .. }) {
                *ip = target.index()
            }
            return ScalarStep::Complete;
        }
        Instruction::Clear { reg: register } | Instruction::UnsetLocal { local: register } => {
            let Some(slot) = slots.get_mut(register.index()) else {
                return ScalarStep::Fallback;
            };
            *slot = None;
            return ScalarStep::Complete;
        }
        _ => return ScalarStep::Fallback,
    };
    let Some(slot) = slots.get_mut(write.destination.index()) else {
        return ScalarStep::Fallback;
    };
    *slot = Some(write.value);
    ScalarStep::Complete
}
