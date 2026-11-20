//! Execute scalar instructions while retaining the current code and register window.
//!
//! A local Arc owns the borrowed instruction slice independently of the VM. The
//! frame IP is published before diagnostics, calls, returns, or suspension.
//!
//! A simple instruction never frees an object: a write over a register that
//! holds an array, an object or a PHP reference falls back. The loop's next
//! tier ([`releasing_step`]) then does the write with the old value taken
//! out, drops it and tests for a due destructor (see `crate::destructors`);
//! a PHP reference goes on to the general handler, which writes through it.
//! So the simple loop itself never tests for due destructors.

use phera_bytecode::{BinaryOp, IncDec, Instruction, Reg};
use phera_value::{Value, to_bool};

use crate::operators::fast_binary;

/// A fast instruction either completes or delegates PHP coercion to the slow handler.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScalarStep {
    Complete,
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

/// A copy of a value read by a simple instruction. The same as
/// `Value::clone`, written out so that it is always inlined into the loop:
/// with two copies of the loop (see `execution_loop`), the derived `clone`
/// has enough callers that the compiler stops inlining it.
#[inline(always)]
pub(crate) fn duplicate(value: &Value) -> Value {
    match value {
        Value::Null => Value::Null,
        Value::Bool(boolean) => Value::Bool(*boolean),
        Value::Int(integer) => Value::Int(*integer),
        Value::Float(float) => Value::Float(*float),
        Value::String(string) => Value::String(string.clone()),
        Value::Array(array) => Value::Array(array.clone()),
        Value::Object(object) => Value::Object(object.clone()),
        Value::Resource(resource) => Value::Resource(resource.clone()),
        Value::Reference(cell) => Value::Reference(cell.clone()),
    }
}

/// Stores an integer result.
///
/// When the register already holds an integer, only the number is replaced.
/// That is the usual case in a loop, and it spares the general "drop the old
/// value, write the new one", which has to ask whether the old value owns
/// memory.
///
/// `GUARDED` (see [`scalar_step`]): a register that holds an array, object
/// or reference is written out of line, where its release is looked at.
#[inline(always)]
fn store_integer<const GUARDED: bool>(
    slots: &mut [Option<Value>],
    register: Reg,
    integer: i64,
) -> ScalarStep {
    match slots.get_mut(register.index()) {
        Some(Some(Value::Int(target))) => *target = integer,
        Some(slot) if GUARDED && owns_heap_values(slot) => {
            return replace(slot, Value::Int(integer));
        }
        Some(slot) => {
            // A register that holds an integer already is the common case in
            // a loop; saying so keeps its test first in the compiled code
            // (measured: two instructions per store otherwise).
            unlikely();
            *slot = Some(Value::Int(integer));
        }
        None => return ScalarStep::Fallback,
    }
    ScalarStep::Complete
}

/// Marks the path that calls it as rarely taken, for the compiler's code
/// layout. It does nothing.
#[cold]
#[inline(always)]
fn unlikely() {}

/// Stores an integer result whatever the register held.
#[inline(always)]
fn overwrite_integer(slots: &mut [Option<Value>], register: Reg, integer: i64) -> ScalarStep {
    match slots.get_mut(register.index()) {
        Some(Some(Value::Int(target))) => *target = integer,
        Some(slot) => *slot = Some(Value::Int(integer)),
        None => return ScalarStep::Fallback,
    }
    ScalarStep::Complete
}

/// Stores any result, dropping what the register held before.
#[inline(always)]
pub(crate) fn store<const GUARDED: bool>(
    slots: &mut [Option<Value>],
    register: Reg,
    value: Value,
) -> ScalarStep {
    match slots.get_mut(register.index()) {
        Some(slot) if GUARDED && owns_heap_values(slot) => replace(slot, value),
        Some(slot) => {
            *slot = Some(value);
            ScalarStep::Complete
        }
        None => ScalarStep::Fallback,
    }
}

/// Overwrites a register that holds a value, unless dropping that value
/// could free an object. Out of line, so that the common writes (into an
/// empty register, or an integer over an integer) stay as short as they
/// can be.
#[cold]
#[inline(never)]
fn replace(slot: &mut Option<Value>, value: Value) -> ScalarStep {
    write_releasing(slot, Some(value))
}

/// Empties a register, unless dropping what it holds could free an object.
#[cold]
#[inline(never)]
fn clear(slot: &mut Option<Value>) -> ScalarStep {
    write_releasing(slot, None)
}

/// Writes a register that holds a value. An object that this was the last
/// handle on is freed here; if that made a destructor due, the answer is
/// `Fallback`, and the general handler runs the instruction once more (the
/// stores that come here write the same value again) and then the
/// destructor. A last handle on an array or PHP reference falls back
/// before the write: what it frees cannot be told from here.
#[inline(always)]
fn write_releasing(slot: &mut Option<Value>, value: Option<Value>) -> ScalarStep {
    let pool = match slot {
        Some(Value::Object(object)) if std::rc::Rc::strong_count(object) == 1 => object.pool(),
        Some(Value::Array(_) | Value::Resource(_) | Value::Reference(_)) if frees_on_drop(slot) => {
            return ScalarStep::Fallback;
        }
        _ => None,
    };
    *slot = value;
    match pool {
        Some(pool) if pool.has_doomed() => ScalarStep::Fallback,
        _ => ScalarStep::Complete,
    }
}

/// Whether a register holds an array, object, resource or PHP reference,
/// whose release can queue a callback. The four kinds are adjacent in `Value`,
/// which makes this test a single comparison.
#[inline(always)]
pub(crate) fn owns_heap_values(slot: &Option<Value>) -> bool {
    matches!(
        slot,
        Some(Value::Array(_) | Value::Object(_) | Value::Resource(_) | Value::Reference(_))
    )
}

/// Whether dropping what a register holds could free an object: it holds
/// the last handle on an array, object or PHP reference. Dropping a shared
/// one only lowers its count.
#[inline(always)]
fn frees_on_drop(slot: &Option<Value>) -> bool {
    match slot {
        Some(Value::Array(array)) => std::rc::Rc::strong_count(array) == 1,
        Some(Value::Object(object)) => std::rc::Rc::strong_count(object) == 1,
        Some(Value::Resource(resource)) => std::rc::Rc::strong_count(resource) == 1,
        Some(Value::Reference(cell)) => std::rc::Rc::strong_count(cell) == 1,
        _ => false,
    }
}

/// Builtins with no coercion can read the local directly.
#[inline(never)]
fn builtin_step<const GUARDED: bool>(
    operation: phera_bytecode::BuiltinUnary,
    destination: Reg,
    source: Reg,
    slots: &mut [Option<Value>],
) -> ScalarStep {
    let Some(Some(value)) = slots.get(source.index()) else {
        return ScalarStep::Fallback;
    };
    match crate::builtin_calls::builtin_value(operation, value) {
        Some(Value::Int(value)) => store_integer::<GUARDED>(slots, destination, value),
        Some(value) => store::<GUARDED>(slots, destination, value),
        None => ScalarStep::Fallback,
    }
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
fn integer_binary<const GUARDED: bool>(
    slots: &mut [Option<Value>],
    operator: BinaryOp,
    destination: Reg,
    left: i64,
    right: i64,
) -> ScalarStep {
    if let Some(result) = integer_arithmetic(operator, left, right) {
        return store_integer::<GUARDED>(slots, destination, result);
    }
    match integer_condition(operator, left, right) {
        Some(condition) => store::<GUARDED>(slots, destination, Value::Bool(condition)),
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
pub(crate) fn scalar_step<const GUARDED: bool>(
    instruction: &Instruction,
    slots: &mut [Option<Value>],
    ip: &mut usize,
) -> ScalarStep {
    match *instruction {
        Instruction::Nop => ScalarStep::Complete,
        Instruction::LoadInt { dst, value } => {
            store_integer::<GUARDED>(slots, dst, i64::from(value))
        }
        Instruction::LoadNull { dst } => store::<GUARDED>(slots, dst, Value::Null),
        Instruction::LoadBool { dst, value } => store::<GUARDED>(slots, dst, Value::Bool(value)),
        Instruction::BuiltinUnary { op, dst, src } => builtin_step::<GUARDED>(op, dst, src, slots),
        Instruction::Unary {
            op: phera_bytecode::UnaryOp::Negate,
            dst,
            src,
        } => {
            let Some(negative) = integer_at(slots, src).and_then(i64::checked_neg) else {
                return ScalarStep::Fallback;
            };
            store_integer::<GUARDED>(slots, dst, negative)
        }
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
            store::<GUARDED>(slots, dst, Value::Bool(value))
        }
        Instruction::Move { dst, src } => {
            if let Some(integer) = integer_at(slots, src) {
                return store_integer::<GUARDED>(slots, dst, integer);
            }
            let Some(Some(value)) = slots.get(src.index()) else {
                return ScalarStep::Fallback;
            };
            let value = duplicate(&value.deref());
            store::<GUARDED>(slots, dst, value)
        }
        Instruction::Assign { local, src } => {
            // Assigning to a PHP reference writes through it: general handler.
            if matches!(slots.get(local.index()), Some(Some(Value::Reference(_)))) {
                return ScalarStep::Fallback;
            }
            if let Some(integer) = integer_at(slots, src) {
                return store_integer::<GUARDED>(slots, local, integer);
            }
            let Some(Some(value)) = slots.get(src.index()) else {
                return ScalarStep::Fallback;
            };
            let value = duplicate(&value.deref());
            store::<GUARDED>(slots, local, value)
        }
        Instruction::AppendLocal { local, src } => {
            let Some(Some(value)) = slots.get(src.index()) else {
                return ScalarStep::Fallback;
            };
            let value = duplicate(&value.deref());
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
                return integer_binary::<GUARDED>(slots, op, dst, left, right);
            }
            let Some(Some(left)) = slots.get(lhs.index()) else {
                return ScalarStep::Fallback;
            };
            match fast_binary(op, left, &Value::Int(right)) {
                Some(value) => store::<GUARDED>(slots, dst, value),
                None => ScalarStep::Fallback,
            }
        }
        Instruction::Binary { op, dst, lhs, rhs } => {
            if let Some(left) = integer_at(slots, lhs)
                && let Some(right) = integer_at(slots, rhs)
            {
                return integer_binary::<GUARDED>(slots, op, dst, left, right);
            }
            let (Some(Some(left)), Some(Some(right))) =
                (slots.get(lhs.index()), slots.get(rhs.index()))
            else {
                return ScalarStep::Fallback;
            };
            match fast_binary(op, left, right) {
                Some(value) => store::<GUARDED>(slots, dst, value),
                None => ScalarStep::Fallback,
            }
        }
        Instruction::AssignOp { op, local, src } => {
            if let Some(left) = integer_at(slots, local)
                && let Some(right) = integer_at(slots, src)
            {
                return match integer_arithmetic(op, left, right) {
                    Some(result) => store_integer::<GUARDED>(slots, local, result),
                    None => ScalarStep::Fallback,
                };
            }
            let (Some(Some(left)), Some(Some(right))) =
                (slots.get(local.index()), slots.get(src.index()))
            else {
                return ScalarStep::Fallback;
            };
            match fast_binary(op, left, right) {
                Some(value) => store::<GUARDED>(slots, local, value),
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
            if store_integer::<GUARDED>(slots, local, new) == ScalarStep::Fallback {
                return ScalarStep::Fallback;
            }
            // The variable has changed, so this store must not fall back.
            // The result goes to a temporary that never holds an object;
            // were it to, its destructor would run at the next check.
            overwrite_integer(slots, dst, result)
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
        // Unsetting a variable that holds an array, object or reference
        // goes to the general handler: it forgets a global bound to it, and
        // a generator released there closes at once.
        Instruction::UnsetLocal { local } => match slots.get_mut(local.index()) {
            Some(slot)
                if slot.as_ref().is_none_or(|value| {
                    !matches!(
                        value,
                        Value::Object(_) | Value::Array(_) | Value::Reference(_)
                    )
                }) =>
            {
                *slot = None;
                ScalarStep::Complete
            }
            _ => ScalarStep::Fallback,
        },
        Instruction::Clear { reg: register } => match slots.get_mut(register.index()) {
            Some(slot) if GUARDED && owns_heap_values(slot) => clear(slot),
            Some(slot) => {
                *slot = None;
                ScalarStep::Complete
            }
            None => ScalarStep::Fallback,
        },
        _ => ScalarStep::Fallback,
    }
}

/// Only integer comparisons have a direct boolean result; other operations use
/// fast_binary so overflow, float results and PHP coercion keep their old paths.
#[inline(always)]
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
pub(crate) fn array_index_step<const GUARDED: bool>(
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
    let value = duplicate(&value.deref());
    let Some(target) = slots.get_mut(destination.index()) else {
        return ScalarStep::Fallback;
    };
    if GUARDED && frees_on_drop(target) {
        return ScalarStep::Fallback;
    }
    *target = Some(value);
    ScalarStep::Complete
}

/// Runs a simple store that fell back only because the register it writes
/// holds an array or an object: the old value is taken out and returned,
/// for the caller to drop and then look for a due destructor. `None` (and
/// nothing changed) for any other instruction or case; the general handler
/// then runs it. It does its few writes itself rather than through
/// [`scalar_step`], whose code is best left with a single caller.
pub(crate) fn releasing_step(
    instruction: Instruction,
    slots: &mut [Option<Value>],
) -> Option<Value> {
    let (register, value) = match instruction {
        Instruction::LoadInt { dst, value } => (dst, Some(Value::Int(i64::from(value)))),
        Instruction::LoadNull { dst } => (dst, Some(Value::Null)),
        Instruction::LoadBool { dst, value } => (dst, Some(Value::Bool(value))),
        Instruction::Clear { reg } => (reg, None),
        Instruction::UnsetLocal { local } => (local, None),
        Instruction::Move { dst, src } | Instruction::Assign { local: dst, src } => {
            // A PHP reference or a missing value needs the general handler.
            let value = match slots.get(src.index()) {
                Some(Some(Value::Reference(_))) | Some(None) | None => return None,
                Some(Some(value)) => value.clone(),
            };
            (dst, Some(value))
        }
        _ => return None,
    };
    let slot = slots.get_mut(register.index())?;
    if !matches!(
        slot,
        Some(Value::Array(_) | Value::Object(_) | Value::Resource(_))
    ) || !frees_on_drop(slot)
    {
        return None;
    }
    std::mem::replace(slot, value)
}
