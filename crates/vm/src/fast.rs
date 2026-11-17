//! Execute scalar instructions while retaining the current code and register window.
//!
//! A local Arc owns the borrowed instruction slice independently of the VM. The
//! frame IP is published before diagnostics, calls, returns, or suspension.

use phera_bytecode::{IncDec, Instruction, Reg};
use phera_value::{Value, to_bool};

use crate::operators::fast_binary;

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
        Instruction::Cast {
            to: phera_bytecode::CastKind::Bool,
            dst: destination,
            src: source,
        } => {
            // Already boolean operands need no conversion. Undefined
            // values and diagnostic-producing casts retain the full handler.
            let Some(Some(Value::Bool(value))) = slots.get(source.index()) else {
                return ScalarStep::Fallback;
            };
            ScalarWrite::new(destination, Value::Bool(*value))
        }
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
            // Integer comparisons produce a branch condition directly,
            // without constructing and dropping an intermediate PHP value.
            if let Value::Int(left) = left
                && let Some(condition) = integer_condition(operator, *left, right as i64)
            {
                if !condition {
                    *ip = target.index();
                }
                return ScalarStep::Complete;
            }
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
        Instruction::Assign { local, src: source } => {
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
    // Replacing one integer payload avoids heap-value drop
    // dispatch and rewriting the enum tag on the scalar loop's common path.
    match (slot, write.value) {
        (Some(Value::Int(target)), Value::Int(value)) => *target = value,
        (target, value) => *target = Some(value),
    }
    ScalarStep::Complete
}

/// Only integer comparisons have a direct boolean result; other operations use
/// fast_binary so overflow, float results and PHP coercion keep their old paths.
#[inline]
fn integer_condition(operator: phera_bytecode::BinaryOp, left: i64, right: i64) -> Option<bool> {
    use phera_bytecode::BinaryOp;
    Some(match operator {
        BinaryOp::Less => left < right,
        BinaryOp::LessEqual => left <= right,
        BinaryOp::Greater => left > right,
        BinaryOp::GreaterEqual => left >= right,
        BinaryOp::Equal | BinaryOp::Identical => left == right,
        BinaryOp::NotEqual | BinaryOp::NotIdentical => left != right,
        _ => return None,
    })
}

/// Existing integer keys need no conversion or diagnostic work.
/// Keep this out of scalar_step's arithmetic dispatch; every failed guard uses
/// the normal PHP offset/reference/diagnostic handler.
#[inline]
pub(crate) fn array_index_step(
    destination: Reg,
    base: Reg,
    index: Reg,
    slots: &mut [Option<Value>],
) -> ScalarStep {
    let Some(Some(Value::Array(array))) = slots.get(base.index()) else {
        return ScalarStep::Fallback;
    };
    let Some(Some(Value::Int(index))) = slots.get(index.index()) else {
        return ScalarStep::Fallback;
    };
    let Some(value) = crate::array_slots::read_integer(array, *index) else {
        return ScalarStep::Fallback;
    };
    let value = value.deref().clone();
    let Some(target) = slots.get_mut(destination.index()) else {
        return ScalarStep::Fallback;
    };
    *target = Some(value);
    ScalarStep::Complete
}
