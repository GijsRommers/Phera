//! Simple instructions on values other than integers, inside the
//! interpreter loop.
//!
//! ```php
//! if ($name === 'id') { ... }        // Binary Identical on two strings
//! $label = $prefix . $name;           // Binary Concat
//! $timeout = $options['timeout'] ?? 30;  // FetchIndex with a string key, JumpIfNull
//! if (!$done) { ... }                 // Unary Not
//! $items = [];                        // NewArray
//! $key = (string) $id;                // Cast to string, of an integer or a string
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
//! PHP code. `===` and `!==` take any operands: identity compares objects
//! by identity and arrays element by element, and runs no PHP code. The operator runs with a sink that only notes whether it
//! raised a warning, a notice or a deprecation, or asked for the request's
//! `precision` setting (a float turned into a string). If it did, or it
//! failed (`"abc" * 2` throws), the result is dropped before anything is
//! written and the general handler runs the instruction again with every
//! PHP rule. Doing the operator twice is safe because on these operands it
//! changes nothing.

use std::cell::Cell;
use std::rc::Rc;

use phera_bytecode::{BinaryOp, CastKind, FetchMode, Instruction, Reg, Target, UnaryOp};
use phera_value::{ArrayKey, Diagnostics, PhpArray, Value, to_bool};

use crate::fast::{ScalarStep, store};

/// `(string) $value` for an integer or a string: the digits as PHP writes
/// them, or the string itself. Any other operand (a float follows the
/// precision setting, an array warns, an object may run `__toString`)
/// converts in the general handler.
fn cast_to_string(operand: Option<&Option<Value>>) -> Option<Value> {
    match operand? {
        Some(Value::Int(integer)) => {
            phera_value::to_php_string(&Value::Int(*integer), &mut phera_value::NoDiagnostics)
                .ok()
                .map(Value::String)
        }
        Some(Value::String(text)) => Some(Value::String(text.clone())),
        Some(Value::InternedStr(text)) => Some(Value::InternedStr(text)),
        _ => None,
    }
}

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
            let left = left.deref();
            let right = right.deref();
            let result = crate::operators::fast_numeric_binary(op, &left, &right)
                .or_else(|| quiet_binary(op, &left, &right));
            drop(left);
            drop(right);
            match result {
                Some(value) => store::<GUARDED, _>(slots, dst, value),
                None => ScalarStep::Fallback,
            }
        }
        Instruction::BinaryInt { op, dst, lhs, rhs } => {
            let Some(Some(left)) = slots.get(lhs.index()) else {
                return ScalarStep::Fallback;
            };
            let left = left.deref();
            let right = Value::Int(i64::from(rhs));
            let result = crate::operators::fast_numeric_binary(op, &left, &right)
                .or_else(|| quiet_binary(op, &left, &right));
            drop(left);
            match result {
                Some(value) => store::<GUARDED, _>(slots, dst, value),
                None => ScalarStep::Fallback,
            }
        }
        Instruction::AssignOp {
            op: BinaryOp::Concat,
            local,
            src,
        } if local != src => assign_concat_plain(slots, local, src),
        Instruction::AssignOp { op, local, src } if op != BinaryOp::Concat => {
            let (Some(Some(left)), Some(Some(right))) =
                (slots.get(local.index()), slots.get(src.index()))
            else {
                return ScalarStep::Fallback;
            };
            if left.is_reference() {
                return ScalarStep::Fallback;
            }
            let right = right.deref();
            let result = crate::operators::fast_numeric_binary(op, left, &right)
                .or_else(|| quiet_binary(op, left, &right));
            drop(right);
            match result {
                Some(value) => store::<GUARDED, _>(slots, local, value),
                None => ScalarStep::Fallback,
            }
        }
        Instruction::BinaryBranch {
            op,
            lhs,
            rhs,
            target,
        } => {
            let (Some(Some(left)), Some(Some(right))) =
                (slots.get(lhs.index()), slots.get(rhs.index()))
            else {
                return ScalarStep::Fallback;
            };
            branch_on_binary::<false>(op, &left.deref(), &right.deref(), target, ip)
        }
        Instruction::BinaryBranchIf {
            op,
            lhs,
            rhs,
            target,
        } => {
            let (Some(Some(left)), Some(Some(right))) =
                (slots.get(lhs.index()), slots.get(rhs.index()))
            else {
                return ScalarStep::Fallback;
            };
            branch_on_binary::<true>(op, &left.deref(), &right.deref(), target, ip)
        }
        Instruction::BinaryIntBranch {
            op,
            lhs,
            rhs,
            target,
        } => {
            let Some(Some(left)) = slots.get(lhs.index()) else {
                return ScalarStep::Fallback;
            };
            branch_on_binary::<false>(op, &left.deref(), &Value::Int(i64::from(rhs)), target, ip)
        }
        Instruction::BinaryIntBranchIf {
            op,
            lhs,
            rhs,
            target,
        } => {
            let Some(Some(left)) = slots.get(lhs.index()) else {
                return ScalarStep::Fallback;
            };
            branch_on_binary::<true>(op, &left.deref(), &Value::Int(i64::from(rhs)), target, ip)
        }
        Instruction::Unary { op, dst, src } => {
            let Some(Some(value)) = slots.get(src.index()) else {
                return ScalarStep::Fallback;
            };
            let result = quiet_unary(op, &value.deref());
            match result {
                Some(value) => store::<GUARDED, _>(slots, dst, value),
                None => ScalarStep::Fallback,
            }
        }
        Instruction::Cast {
            to: CastKind::String,
            dst,
            src,
        } => match cast_to_string(slots.get(src.index())) {
            Some(text) => store::<GUARDED, _>(slots, dst, text),
            None => ScalarStep::Fallback,
        },
        Instruction::Cast {
            to: CastKind::Int,
            dst,
            src,
        } => match slots.get(src.index()) {
            Some(Some(Value::Float(value)))
                if value.is_finite()
                    && *value >= -9223372036854775808.0
                    && *value < 9223372036854775808.0 =>
            {
                let value = *value as i64;
                store::<GUARDED, _>(slots, dst, Value::Int(value))
            }
            _ => ScalarStep::Fallback,
        },
        Instruction::Cast {
            to: CastKind::Float,
            dst,
            src,
        } => match slots.get(src.index()) {
            Some(Some(Value::Int(value))) => {
                let value = *value as f64;
                store::<GUARDED, _>(slots, dst, Value::Float(value))
            }
            _ => ScalarStep::Fallback,
        },
        Instruction::JumpIfNull { value, target } => jump_on_null(slots, value, target, true, ip),
        Instruction::JumpIfNotNull { value, target } => {
            jump_on_null(slots, value, target, false, ip)
        }
        Instruction::NewArray { dst, capacity } => {
            let array = if capacity > 0 {
                PhpArray::with_capacity(capacity as usize)
            } else {
                PhpArray::new()
            };
            store::<GUARDED, _>(slots, dst, Value::Array(Rc::new(array)))
        }
        Instruction::ArraySet { array, key, src } => array_set(slots, array, key, src),
        Instruction::ArrayPush { array, src } => array_push(slots, array, src),
        Instruction::ArrayKeyExists { dst, key, array } => {
            array_key_exists::<GUARDED>(slots, dst, key, array)
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
                Some(value) => store::<GUARDED, _>(slots, dst, value),
                None => ScalarStep::Fallback,
            }
        }
        _ => ScalarStep::Fallback,
    }
}

/// `$local .= $src` in-place for plain non-reference strings without leaving
/// `Machine::run` or allocating a temporary string copy of `$src`.
#[inline(always)]
fn assign_concat_plain(slots: &mut [Option<Value>], local: Reg, src: Reg) -> ScalarStep {
    let interned: Option<&'static [u8]> = match slots.get(src.index()) {
        Some(Some(Value::InternedStr(s))) => Some(s.as_bytes()),
        Some(Some(Value::String(_))) => None,
        _ => return ScalarStep::Fallback,
    };
    if let Some(bytes) = interned {
        match slots.get_mut(local.index()) {
            Some(Some(slot @ Value::InternedStr(_))) => {
                let Value::InternedStr(existing) = *slot else {
                    unreachable!()
                };
                let mut owned = existing.to_php_string();
                owned.push_bytes(bytes);
                *slot = Value::String(owned);
                ScalarStep::Complete
            }
            Some(Some(Value::String(existing))) => {
                existing.push_bytes(bytes);
                ScalarStep::Complete
            }
            _ => ScalarStep::Fallback,
        }
    } else {
        let Some(Some(Value::String(src_str))) = slots.get(src.index()) else {
            return ScalarStep::Fallback;
        };
        let src_cloned = src_str.clone();
        match slots.get_mut(local.index()) {
            Some(Some(slot @ Value::InternedStr(_))) => {
                let Value::InternedStr(existing) = *slot else {
                    unreachable!()
                };
                let mut owned = existing.to_php_string();
                owned.push_bytes(src_cloned.as_bytes());
                *slot = Value::String(owned);
                ScalarStep::Complete
            }
            Some(Some(Value::String(existing))) => {
                existing.push_bytes(src_cloned.as_bytes());
                ScalarStep::Complete
            }
            _ => ScalarStep::Fallback,
        }
    }
}

/// `ArraySet`: `$array[$key] = $src` during array literal construction when
/// `array` is an array, `key` is an integer or string, and `src` is defined.
#[inline(always)]
fn array_set(slots: &mut [Option<Value>], array: Reg, key: Reg, src: Reg) -> ScalarStep {
    let array_key = match slots.get(key.index()) {
        Some(Some(Value::Int(integer))) => ArrayKey::Int(*integer),
        Some(Some(Value::InternedStr(string))) => match string.numeric_key() {
            Some(integer) => ArrayKey::Int(integer),
            None => ArrayKey::Interned(string),
        },
        Some(Some(Value::String(string))) => ArrayKey::from_string(string.clone()),
        _ => return ScalarStep::Fallback,
    };
    let Some(Some(source)) = slots.get(src.index()) else {
        return ScalarStep::Fallback;
    };
    let value = source.deref().clone();
    let Some(Some(Value::Array(target))) = slots.get_mut(array.index()) else {
        return ScalarStep::Fallback;
    };
    Rc::make_mut(target).insert(array_key, value);
    ScalarStep::Complete
}

/// `ArrayPush`: `$array[] = $src` during array literal construction when
/// `array` is an array and `src` is defined.
#[inline(always)]
fn array_push(slots: &mut [Option<Value>], array: Reg, src: Reg) -> ScalarStep {
    let Some(Some(source)) = slots.get(src.index()) else {
        return ScalarStep::Fallback;
    };
    let value = source.deref().clone();
    let Some(Some(Value::Array(target))) = slots.get_mut(array.index()) else {
        return ScalarStep::Fallback;
    };
    match Rc::make_mut(target).push(value) {
        Ok(_) => ScalarStep::Complete,
        Err(_) => ScalarStep::Fallback,
    }
}

/// `ArrayKeyExists`: `array_key_exists($key, $array)` for an array and plain key.
#[inline(always)]
fn array_key_exists<const GUARDED: bool>(
    slots: &mut [Option<Value>],
    dst: Reg,
    key: Reg,
    array: Reg,
) -> ScalarStep {
    let found = {
        let (Some(Some(key_val)), Some(Some(array_val))) =
            (slots.get(key.index()), slots.get(array.index()))
        else {
            return ScalarStep::Fallback;
        };
        let array_ref = array_val.deref();
        let Value::Array(array) = &*array_ref else {
            return ScalarStep::Fallback;
        };
        let key_ref = key_val.deref();
        match &*key_ref {
            Value::Int(integer) => array.get_int(*integer).is_some(),
            Value::Bool(boolean) => array.get_int(i64::from(*boolean)).is_some(),
            Value::InternedStr(string) => array.contains_interned(string),
            Value::String(string) => array.get_str(string).is_some(),
            _ => return ScalarStep::Fallback,
        }
    };
    store::<GUARDED, _>(slots, dst, Value::Bool(found))
}

/// `JumpIfNull` (`when_null`) or `JumpIfNotNull`. An unset register falls
/// back: the general handler reads it as null, warning when it is a named
/// local, as in PHP for `$undef === null` (a `??` always tests a register a
/// quiet read filled).
#[inline(always)]
fn jump_on_null(
    slots: &[Option<Value>],
    register: Reg,
    target: Target,
    when_null: bool,
    ip: &mut usize,
) -> ScalarStep {
    let Some(Some(value)) = slots.get(register.index()) else {
        return ScalarStep::Fallback;
    };
    let is_null = value.deref().is_null();
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
        FetchMode::Read | FetchMode::Destructure => false,
        FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty => true,
    };
    let key = slots.get(index.index())?.as_ref()?;
    if !matches!(
        key,
        Value::Int(_) | Value::InternedStr(_) | Value::String(_)
    ) {
        return None;
    }
    match slots.get(base.index())? {
        Some(Value::Array(array)) => array_lookup_key(array, key, quiet),
        Some(Value::Reference(cell)) => {
            let borrowed = cell.try_borrow().ok()?;
            match &*borrowed {
                Value::Array(array) => array_lookup_key(array, key, quiet),
                _ => None,
            }
        }
        Some(Value::InternedStr(string)) if mode != FetchMode::Destructure => {
            string_offset_element(string.as_bytes(), key, quiet)
        }
        Some(Value::String(string)) if mode != FetchMode::Destructure => {
            string_offset_element(string.as_bytes(), key, quiet)
        }
        None | Some(Value::Null) if quiet => Some(Value::Null),
        _ => None,
    }
}

#[inline(always)]
fn array_lookup_key(array: &PhpArray, key: &Value, quiet: bool) -> Option<Value> {
    let found = match key {
        Value::Int(integer) => array.get_int(*integer),
        Value::InternedStr(string) => array.get_interned(string),
        Value::String(string) => array.get_str(string),
        _ => unreachable!("checked above"),
    };
    match found {
        Some(value) => Some(crate::fast::duplicate(&value.deref())),
        None if quiet => Some(Value::Null),
        None => None,
    }
}

/// `$string[$int_index]` when the index is an integer: returns the single-byte
/// string, or `null` in quiet mode when out of bounds. Out-of-bounds reads fall
/// back for PHP's warning.
#[inline(always)]
fn string_offset_element(bytes: &[u8], key: &Value, quiet: bool) -> Option<Value> {
    let Value::Int(index) = *key else {
        return None;
    };
    let from_start = if index < 0 {
        (bytes.len() as i64).checked_add(index)?
    } else {
        index
    };
    match usize::try_from(from_start)
        .ok()
        .and_then(|pos| bytes.get(pos))
    {
        Some(&byte) => Some(Value::from(vec![byte])),
        None if quiet => Some(Value::Null),
        None => None,
    }
}

/// Whether a value is one the value layer's operators handle alone.
#[inline(always)]
fn is_plain(value: &Value) -> bool {
    matches!(
        value,
        Value::Null
            | Value::Bool(_)
            | Value::Int(_)
            | Value::Float(_)
            | Value::InternedStr(_)
            | Value::String(_)
    )
}

/// A binary operator on two plain values, if it raises nothing.
pub(crate) fn quiet_binary(operator: BinaryOp, left: &Value, right: &Value) -> Option<Value> {
    // Identity runs no PHP code on any operands: objects are compared by
    // identity, arrays element by element with `===`.
    let identity = matches!(operator, BinaryOp::Identical | BinaryOp::NotIdentical);
    if !identity && (!is_plain(left) || !is_plain(right)) {
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

/// Run a comparison in the value tier and jump when its truthiness equals WHEN.
#[inline(always)]
fn branch_on_binary<const WHEN: bool>(
    op: BinaryOp,
    left: &Value,
    right: &Value,
    target: Target,
    ip: &mut usize,
) -> ScalarStep {
    match crate::operators::fast_numeric_binary(op, left, right)
        .or_else(|| quiet_binary(op, left, right))
    {
        Some(value) => {
            if to_bool(&value) == WHEN {
                *ip = target.index();
            }
            ScalarStep::Complete
        }
        None => ScalarStep::Fallback,
    }
}
