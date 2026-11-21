//! Simple instructions on values other than integers, inside the
//! interpreter loop.
//!
//! ```php
//! if ($name === 'id') { ... }        // Binary Identical on two strings
//! $label = $prefix . $name;           // Binary Concat
//! $timeout = $options['timeout'] ?? 30;  // FetchIndex with a string key, JumpIfNull
//! if (!$done) { ... }                 // Unary Not
//! $items = [];                        // NewArray
//! ```
//!
//! `crate::fast` does integer instructions inline in the loop. These are
//! the next most common simple instructions in framework code. Leaving the
//! loop for one of them costs several hundred machine instructions (the
//! loop's state is stored, the general handler finds the frame again, and
//! the loop starts over), so they are done here, in one function kept out
//! of line so that the loop's own code stays as it is.
//!
//! An operator is taken here only when both operands are null, booleans,
//! integers, floats or strings, because then nothing but the value layer's
//! own operator is involved: no `__toString`, no comparison of objects, no
//! PHP code. The operator runs with a sink that only notes whether it
//! raised a warning, a notice or a deprecation, or asked for the request's
//! `precision` setting (a float turned into a string). If it did, or it
//! failed (`"abc" * 2` throws), the result is dropped before anything is
//! written and the general handler runs the instruction again with every
//! PHP rule. Doing the operator twice is safe because on these operands it
//! changes nothing.

use std::cell::Cell;
use std::rc::Rc;

use phera_bytecode::{BinaryOp, FetchMode, Instruction, Reg, Target, UnaryOp};
use phera_value::{ArrayKey, Diagnostics, PhpArray, Value, to_bool};

use crate::fast::{ScalarStep, store};

/// Runs `instruction` if it is one of the cases above; `Fallback`, with
/// nothing changed, for anything else.
#[inline(never)]
pub(crate) fn value_step<const GUARDED: bool>(
    instruction: &Instruction,
    slots: &mut [Option<Value>],
    ip: &mut usize,
) -> ScalarStep {
    match *instruction {
        Instruction::Binary { op, dst, lhs, rhs } => {
            let (Some(Some(left)), Some(Some(right))) =
                (slots.get(lhs.index()), slots.get(rhs.index()))
            else {
                return ScalarStep::Fallback;
            };
            let result = quiet_binary(op, &left.deref(), &right.deref());
            match result {
                Some(value) => store::<GUARDED>(slots, dst, value),
                None => ScalarStep::Fallback,
            }
        }
        Instruction::Unary { op, dst, src } => {
            let Some(Some(value)) = slots.get(src.index()) else {
                return ScalarStep::Fallback;
            };
            let result = quiet_unary(op, &value.deref());
            match result {
                Some(value) => store::<GUARDED>(slots, dst, value),
                None => ScalarStep::Fallback,
            }
        }
        Instruction::JumpIfNull { value, target } => jump_on_null(slots, value, target, true, ip),
        Instruction::JumpIfNotNull { value, target } => {
            jump_on_null(slots, value, target, false, ip)
        }
        Instruction::NewArray { dst, .. } => {
            store::<GUARDED>(slots, dst, Value::Array(Rc::new(PhpArray::new())))
        }
        Instruction::FetchIndex {
            dst,
            base,
            index,
            mode,
            ..
        } => {
            let element = array_element(slots, base, index, mode);
            match element {
                Some(value) => store::<GUARDED>(slots, dst, value),
                None => ScalarStep::Fallback,
            }
        }
        _ => ScalarStep::Fallback,
    }
}

/// `JumpIfNull` (`when_null`) or `JumpIfNotNull`. An unset register reads
/// as null without a warning, as in the general handler: these follow a
/// quiet read (`??`, `?->`).
#[inline(always)]
fn jump_on_null(
    slots: &[Option<Value>],
    register: Reg,
    target: Target,
    when_null: bool,
    ip: &mut usize,
) -> ScalarStep {
    let Some(slot) = slots.get(register.index()) else {
        return ScalarStep::Fallback;
    };
    let is_null = slot.as_ref().is_none_or(|value| value.deref().is_null());
    if is_null == when_null {
        *ip = target.index();
    }
    ScalarStep::Complete
}

/// `$array[$key]` with an integer or string key. A read gives an element
/// that is present (a missing one warns). A quiet read (`??`, `isset`,
/// `empty`) gives null for a missing element, or for an unset or null
/// variable, without a diagnostic, as the general handler does; what is
/// then tested of the value is the next instruction's business. A string
/// offset, an object with `ArrayAccess`, any other key type: `None`.
#[inline(always)]
fn array_element(slots: &[Option<Value>], base: Reg, index: Reg, mode: FetchMode) -> Option<Value> {
    let quiet = match mode {
        FetchMode::Read => false,
        FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty => true,
        FetchMode::Destructure => return None,
    };
    let key = match slots.get(index.index())? {
        Some(Value::Int(integer)) => ArrayKey::Int(*integer),
        Some(Value::String(string)) => ArrayKey::from_string(string.clone()),
        _ => return None,
    };
    match slots.get(base.index())? {
        Some(Value::Array(array)) => match array.get(&key) {
            Some(value) => Some(value.deref().clone()),
            None if quiet => Some(Value::Null),
            None => None,
        },
        None | Some(Value::Null) if quiet => Some(Value::Null),
        _ => None,
    }
}

/// Whether a value is one the value layer's operators handle alone.
#[inline(always)]
fn is_plain(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_)
    )
}

/// A binary operator on two plain values, if it raises nothing.
pub(crate) fn quiet_binary(operator: BinaryOp, left: &Value, right: &Value) -> Option<Value> {
    if !is_plain(left) || !is_plain(right) {
        return None;
    }
    let mut sink = Silent::default();
    let value = crate::ops::binary(operator, left, right, &mut sink).ok()?;
    sink.quiet().then_some(value)
}

/// A unary operator on a plain value, if it raises nothing. `!` takes an
/// array too: it only asks whether the array is empty.
fn quiet_unary(operator: UnaryOp, value: &Value) -> Option<Value> {
    if operator == UnaryOp::Not && matches!(value, Value::Array(_)) {
        return Some(Value::Bool(!to_bool(value)));
    }
    if !is_plain(value) {
        return None;
    }
    let mut sink = Silent::default();
    let value = crate::ops::unary(operator, value, &mut sink).ok()?;
    sink.quiet().then_some(value)
}

/// A diagnostics sink that only remembers whether anything was raised, or
/// whether the answer depended on the `precision` setting.
#[derive(Default)]
struct Silent {
    raised: Cell<bool>,
}

impl Silent {
    fn quiet(&self) -> bool {
        !self.raised.get()
    }
}

impl Diagnostics for Silent {
    fn float_precision(&self) -> i32 {
        self.raised.set(true);
        14
    }

    fn warning(&mut self, _message: String) {
        self.raised.set(true);
    }

    fn notice(&mut self, _message: String) {
        self.raised.set(true);
    }

    fn deprecated(&mut self, _message: String) {
        self.raised.set(true);
    }
}
