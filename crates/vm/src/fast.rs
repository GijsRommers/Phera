//! Execute scalar instructions while retaining the current code and register window.
//!
//! A local Arc owns the borrowed instruction slice independently of the VM. The
//! frame IP is published before diagnostics, calls, returns, or suspension.

use phera_bytecode::{IncDec, Instruction};
use phera_value::{Value, to_bool};

use crate::operators::fast_binary;

/// Scalar cases avoid repeated frame lookup; coercions and aliases use handlers.
#[inline]
pub(crate) fn scalar_step(
    instruction: Instruction,
    slots: &mut [Option<Value>],
    ip: &mut usize,
    tracks_arguments: bool,
) -> bool {
    let (destination, value) = match instruction {
        Instruction::Nop => return true,
        Instruction::LoadInt { dst, value } => (dst, Value::Int(value as i64)),
        Instruction::LoadNull { dst } => (dst, Value::Null),
        Instruction::LoadBool { dst, value } => (dst, Value::Bool(value)),
        Instruction::Move { dst, src } => {
            let Some(Some(value)) = slots.get(src.index()) else {
                return false;
            };
            (dst, value.deref().clone())
        }
        Instruction::AppendLocal { local, src } => {
            let Some(Some(value)) = slots.get(src.index()) else {
                return false;
            };
            let value = value.deref().clone();
            let Some(Some(Value::Array(array))) = slots.get_mut(local.index()) else {
                return false;
            };
            return std::rc::Rc::make_mut(array).push(value).is_ok();
        }
        Instruction::BinaryIntBranch {
            op,
            lhs,
            rhs,
            target,
        } => {
            let Some(Some(left)) = slots.get(lhs.index()) else {
                return false;
            };
            let Some(value) = fast_binary(op, left, &Value::Int(rhs as i64)) else {
                return false;
            };
            if !to_bool(&value) {
                *ip = target.index()
            }
            return true;
        }
        Instruction::BinaryInt { op, dst, lhs, rhs } => {
            let Some(Some(left)) = slots.get(lhs.index()) else {
                return false;
            };
            let Some(value) = fast_binary(op, left, &Value::Int(rhs as i64)) else {
                return false;
            };
            (dst, value)
        }
        Instruction::Binary { op, dst, lhs, rhs } => {
            let Some(Some(left)) = slots.get(lhs.index()) else {
                return false;
            };
            let Some(Some(right)) = slots.get(rhs.index()) else {
                return false;
            };
            let Some(value) = fast_binary(op, left, right) else {
                return false;
            };
            (dst, value)
        }
        Instruction::Assign { local, src } if !tracks_arguments => {
            if matches!(slots.get(local.index()), Some(Some(Value::Reference(_)))) {
                return false;
            }
            let Some(Some(value)) = slots.get(src.index()) else {
                return false;
            };
            (local, value.deref().clone())
        }
        Instruction::AssignOp { op, local, src } => {
            let Some(Some(left)) = slots.get(local.index()) else {
                return false;
            };
            let Some(Some(right)) = slots.get(src.index()) else {
                return false;
            };
            let Some(value) = fast_binary(op, left, right) else {
                return false;
            };
            (local, value)
        }
        Instruction::IncDecLocal { mode, dst, local } => {
            let Some(Some(Value::Int(integer))) = slots.get(local.index()) else {
                return false;
            };
            let old = *integer;
            let increment = matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement);
            let Some(new) = old.checked_add(if increment { 1 } else { -1 }) else {
                return false;
            };
            let Some(slot) = slots.get_mut(local.index()) else {
                return false;
            };
            *slot = Some(Value::Int(new));
            (
                dst,
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
            return true;
        }
        Instruction::JumpIf { condition, target }
        | Instruction::JumpIfNot { condition, target } => {
            let Some(Some(value)) = slots.get(condition.index()) else {
                return false;
            };
            if to_bool(value) == matches!(instruction, Instruction::JumpIf { .. }) {
                *ip = target.index()
            }
            return true;
        }
        Instruction::Clear { reg } | Instruction::UnsetLocal { local: reg } => {
            let Some(slot) = slots.get_mut(reg.index()) else {
                return false;
            };
            *slot = None;
            return true;
        }
        _ => return false,
    };
    let Some(slot) = slots.get_mut(destination.index()) else {
        return false;
    };
    *slot = Some(value);
    true
}
