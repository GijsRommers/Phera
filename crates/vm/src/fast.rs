//! Execute scalar instructions while retaining the current code and register window.
//!
//! A local Arc owns the borrowed instruction slice independently of the VM. The
//! frame IP is published before diagnostics, calls, returns, or suspension.

use phera_bytecode::{BinaryOp, IncDec, Instruction, Reg};
use phera_value::{Value, to_bool};

use crate::operators::fast_binary;

/// A fast instruction either completes or delegates PHP coercion to the slow handler.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScalarStep {
    Complete,
    /// Complete, and the register written held a value that could own an
    /// object: a destructor may now be due (see `crate::destructors`).
    Released,
    Fallback,
}

/// The integer in a register, if that is what it holds. A PHP reference, a
/// float or an unset variable gives `None` and the caller falls back.
#[inline(always)]
fn integer_at(slots: &[Option<Value>], register: Reg) -> Option<i64> {
    match slots.get(register.index()) {
        Some(Some(Value::Int(integer))) => Some(*integer),
        _ => None,
    }
}

/// Stores an integer result.
///
/// When the register already holds an integer, only the number is replaced.
/// That is the usual case in a loop, and it spares the general "drop the old
/// value, write the new one", which has to ask whether the old value owns
/// memory.
#[inline(always)]
fn store_integer(slots: &mut [Option<Value>], register: Reg, integer: i64) -> ScalarStep {
    match slots.get_mut(register.index()) {
        Some(Some(Value::Int(target))) => {
            *target = integer;
            ScalarStep::Complete
        }
        Some(slot) => replace(slot, Value::Int(integer)),
        None => ScalarStep::Fallback,
    }
}

/// Stores any result, dropping what the register held before.
#[inline(always)]
fn store(slots: &mut [Option<Value>], register: Reg, value: Value) -> ScalarStep {
    match slots.get_mut(register.index()) {
        Some(slot) => replace(slot, value),
        None => ScalarStep::Fallback,
    }
}

/// Writes a register and says whether what it held could own an object,
/// whose release may make a destructor due. Only then does the loop test
/// for due destructors, so simple instructions on scalars never do.
#[inline(always)]
fn replace(slot: &mut Option<Value>, value: Value) -> ScalarStep {
    let released = may_own_objects(slot);
    *slot = Some(value);
    if released {
        ScalarStep::Released
    } else {
        ScalarStep::Complete
    }
}

/// Whether dropping what a register holds could free an object.
#[inline(always)]
fn may_own_objects(slot: &Option<Value>) -> bool {
    matches!(
        slot,
        Some(Value::Array(_) | Value::Object(_) | Value::Reference(_))
    )
}

/// `+`, `-`, `*` and `%` on two integers, when the result is an integer.
///
/// `None` sends the instruction to the general handler: an overflow becomes a
/// float there, and `% 0` throws there. `%` by -1 is 0 in PHP (and would
/// overflow in Rust for the smallest integer).
#[inline(always)]
fn integer_arithmetic(operator: BinaryOp, left: i64, right: i64) -> Option<i64> {
    match operator {
        BinaryOp::Add => left.checked_add(right),
        BinaryOp::Subtract => left.checked_sub(right),
        BinaryOp::Multiply => left.checked_mul(right),
        BinaryOp::Modulo if right == -1 => Some(0),
        BinaryOp::Modulo if right != 0 => Some(left % right),
        _ => None,
    }
}

/// A binary operator on two integers: arithmetic gives an integer, a
/// comparison gives a boolean. Anything else falls back.
#[inline(always)]
fn integer_binary(
    slots: &mut [Option<Value>],
    operator: BinaryOp,
    destination: Reg,
    left: i64,
    right: i64,
) -> ScalarStep {
    if let Some(result) = integer_arithmetic(operator, left, right) {
        return store_integer(slots, destination, result);
    }
    match integer_condition(operator, left, right) {
        Some(condition) => store(slots, destination, Value::Bool(condition)),
        None => ScalarStep::Fallback,
    }
}

/// Runs one simple instruction directly on the register window.
///
/// "Simple" means it needs nothing but the registers: no warning can be
/// raised, no PHP code can run, nothing outside the frame is touched.
/// Integers get the shortest path, because loop counters, sums and array
/// indexes are integers. Whatever does not fit (a float, a string, a PHP
/// reference, an unset variable, an overflow) returns `Fallback`, and the
/// general handler for that instruction does it with full PHP semantics.
#[inline(always)]
pub(crate) fn scalar_step(
    instruction: Instruction,
    slots: &mut [Option<Value>],
    ip: &mut usize,
) -> ScalarStep {
    match instruction {
        Instruction::Nop => ScalarStep::Complete,
        Instruction::LoadInt { dst, value } => store_integer(slots, dst, i64::from(value)),
        Instruction::LoadNull { dst } => store(slots, dst, Value::Null),
        Instruction::LoadBool { dst, value } => store(slots, dst, Value::Bool(value)),
        Instruction::Cast {
            to: phera_bytecode::CastKind::Bool,
            dst,
            src,
        } => {
            // Only a value that is a boolean already; a real conversion may warn.
            let Some(Some(Value::Bool(value))) = slots.get(src.index()) else {
                return ScalarStep::Fallback;
            };
            let value = *value;
            store(slots, dst, Value::Bool(value))
        }
        Instruction::Move { dst, src } => {
            if let Some(integer) = integer_at(slots, src) {
                return store_integer(slots, dst, integer);
            }
            let Some(Some(value)) = slots.get(src.index()) else {
                return ScalarStep::Fallback;
            };
            let value = value.deref().clone();
            store(slots, dst, value)
        }
        Instruction::Assign { local, src } => {
            // Assigning to a PHP reference writes through it: general handler.
            if matches!(slots.get(local.index()), Some(Some(Value::Reference(_)))) {
                return ScalarStep::Fallback;
            }
            if let Some(integer) = integer_at(slots, src) {
                return store_integer(slots, local, integer);
            }
            let Some(Some(value)) = slots.get(src.index()) else {
                return ScalarStep::Fallback;
            };
            let value = value.deref().clone();
            store(slots, local, value)
        }
        Instruction::AppendLocal { local, src } => {
            let Some(Some(value)) = slots.get(src.index()) else {
                return ScalarStep::Fallback;
            };
            let value = value.deref().clone();
            let Some(Some(Value::Array(array))) = slots.get_mut(local.index()) else {
                return ScalarStep::Fallback;
            };
            match std::rc::Rc::make_mut(array).push(value) {
                Ok(_) => ScalarStep::Complete,
                Err(_) => ScalarStep::Fallback,
            }
        }
        Instruction::BinaryIntBranch {
            op,
            lhs,
            rhs,
            target,
        } => {
            let right = i64::from(rhs);
            if let Some(left) = integer_at(slots, lhs)
                && let Some(condition) = integer_condition(op, left, right)
            {
                if !condition {
                    *ip = target.index();
                }
                return ScalarStep::Complete;
            }
            let Some(Some(left)) = slots.get(lhs.index()) else {
                return ScalarStep::Fallback;
            };
            let Some(value) = fast_binary(op, left, &Value::Int(right)) else {
                return ScalarStep::Fallback;
            };
            if !to_bool(&value) {
                *ip = target.index();
            }
            ScalarStep::Complete
        }
        Instruction::BinaryInt { op, dst, lhs, rhs } => {
            let right = i64::from(rhs);
            if let Some(left) = integer_at(slots, lhs) {
                return integer_binary(slots, op, dst, left, right);
            }
            let Some(Some(left)) = slots.get(lhs.index()) else {
                return ScalarStep::Fallback;
            };
            match fast_binary(op, left, &Value::Int(right)) {
                Some(value) => store(slots, dst, value),
                None => ScalarStep::Fallback,
            }
        }
        Instruction::Binary { op, dst, lhs, rhs } => {
            if let Some(left) = integer_at(slots, lhs)
                && let Some(right) = integer_at(slots, rhs)
            {
                return integer_binary(slots, op, dst, left, right);
            }
            let (Some(Some(left)), Some(Some(right))) =
                (slots.get(lhs.index()), slots.get(rhs.index()))
            else {
                return ScalarStep::Fallback;
            };
            match fast_binary(op, left, right) {
                Some(value) => store(slots, dst, value),
                None => ScalarStep::Fallback,
            }
        }
        Instruction::AssignOp { op, local, src } => {
            if let Some(left) = integer_at(slots, local)
                && let Some(right) = integer_at(slots, src)
            {
                return match integer_arithmetic(op, left, right) {
                    Some(result) => store_integer(slots, local, result),
                    None => ScalarStep::Fallback,
                };
            }
            let (Some(Some(left)), Some(Some(right))) =
                (slots.get(local.index()), slots.get(src.index()))
            else {
                return ScalarStep::Fallback;
            };
            match fast_binary(op, left, right) {
                Some(value) => store(slots, local, value),
                None => ScalarStep::Fallback,
            }
        }
        Instruction::IncDecLocal { mode, dst, local } => {
            let Some(old) = integer_at(slots, local) else {
                return ScalarStep::Fallback;
            };
            let step = match mode {
                IncDec::PreIncrement | IncDec::PostIncrement => 1,
                IncDec::PreDecrement | IncDec::PostDecrement => -1,
            };
            let Some(new) = old.checked_add(step) else {
                return ScalarStep::Fallback;
            };
            // `$i++` gives the value before the change, `++$i` the one after.
            let result = match mode {
                IncDec::PostIncrement | IncDec::PostDecrement => old,
                IncDec::PreIncrement | IncDec::PreDecrement => new,
            };
            if store_integer(slots, local, new) == ScalarStep::Fallback {
                return ScalarStep::Fallback;
            }
            store_integer(slots, dst, result)
        }
        Instruction::Jump { target } => {
            *ip = target.index();
            ScalarStep::Complete
        }
        Instruction::JumpIf { condition, target }
        | Instruction::JumpIfNot { condition, target } => {
            let Some(Some(value)) = slots.get(condition.index()) else {
                return ScalarStep::Fallback;
            };
            let truthy = match value {
                Value::Bool(boolean) => *boolean,
                other => to_bool(other),
            };
            if truthy == matches!(instruction, Instruction::JumpIf { .. }) {
                *ip = target.index();
            }
            ScalarStep::Complete
        }
        Instruction::Clear { reg: register } | Instruction::UnsetLocal { local: register } => {
            match slots.get_mut(register.index()) {
                Some(slot) => {
                    let released = may_own_objects(slot);
                    *slot = None;
                    if released {
                        ScalarStep::Released
                    } else {
                        ScalarStep::Complete
                    }
                }
                None => ScalarStep::Fallback,
            }
        }
        _ => ScalarStep::Fallback,
    }
}

/// Only integer comparisons have a direct boolean result; other operations use
/// fast_binary so overflow, float results and PHP coercion keep their old paths.
#[inline]
fn integer_condition(operator: BinaryOp, left: i64, right: i64) -> Option<bool> {
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
    replace(target, value)
}
