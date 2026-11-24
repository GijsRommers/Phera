//! Writes of an array element, in the interpreter loop.
//!
//! ```php
//! $row['id'] = $id;                  // AssignPath of a local array
//! $this->items[$key] = $value;       // AssignPath of an array property
//! $total = $map[$a][$b] = $count;    // AssignPathFromLocal, the value kept
//! ```
//!
//! The array is a local variable, or a declared property of `$this` or of
//! an object in a register, reached through the instruction's property
//! site as reads of such elements are (`fast_property_elements`); the VM
//! records that site when it makes the write itself. The write is a plain
//! one (`crate::element_writes`): up to three integer or string keys, every
//! level but the last an array already, the last element not a PHP
//! reference. Anything else answers [`Step::Slow`] with nothing changed.
//!
//! The element's old value is let go of here, so the guarded loop (some
//! object has a destructor due) leaves these writes to the VM.

use std::rc::Rc;

use phera_bytecode::{PathBase, PathId, Reg, Step as PathStep};
use phera_value::{ArrayKey, InternedStr, PhpArray, PhpString, Value};

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::element_writes::{MOST_KEYS, Target, writable_creating, write_creating};
use crate::fast::{duplicate, store};

/// The keys of a write, read from their registers, and whether it appends
/// after them (`[]`).
pub(super) struct Keys {
    keys: [ArrayKey; MOST_KEYS],
    count: usize,
    pub(super) target: Target,
}

impl Keys {
    pub(super) fn as_slice(&self) -> &[ArrayKey] {
        self.keys.get(..self.count).unwrap_or_default()
    }
}

impl Machine<'_> {
    /// `AssignPath`, or with `result` `AssignPathFromLocal`, of an array
    /// element.
    pub(super) fn assign_element<const GUARDED: bool>(
        &mut self,
        position: Position,
        path: PathId,
        (source, result): (Reg, Option<Reg>),
    ) -> Step {
        let base = position.frame.base();
        // The result register is checked first: once the element is
        // written, the value must be stored.
        if result.is_some_and(|result| self.registers.window(base + result.index(), 1).is_empty()) {
            return Step::Slow;
        }
        let value = match self.registers.window(base + source.index(), 1) {
            [Some(value)] => duplicate(&value.deref()),
            _ => return Step::Slow,
        };
        let Some(path_body) = self
            .code
            .function(position.frame.function)
            .paths
            .get(path.index())
        else {
            return Step::Slow;
        };
        let result_value = result.map(|_| duplicate(&value));
        let (written, old) = match (path_body.base, path_body.steps.as_slice()) {
            // Single-index `$arr[$k] = $v` (including reference-wrapped `&$arr`):
            // one lookup, no double value clone, no redundant `writable_creating` pass.
            (PathBase::Register(root), [PathStep::Index(key_reg)]) => {
                let Some(key) = read_index_key(self.registers.window(base + key_reg.index(), 1))
                else {
                    return Step::Slow;
                };
                match self.registers.window_mut(base + root.index(), 1) {
                    [Some(Value::Array(array))] => write_single_index(array, key, value),
                    [Some(Value::Reference(cell))] => match cell.try_borrow_mut() {
                        Ok(mut borrowed) => match &mut *borrowed {
                            Value::Array(array) => write_single_index(array, key, value),
                            _ => (false, None),
                        },
                        Err(_) => (false, None),
                    },
                    _ => (false, None),
                }
            }
            (PathBase::Register(root), [PathStep::Index(_) | PathStep::Append, ..]) => {
                let Some(keys) = self.write_keys(position, &path_body.steps) else {
                    return Step::Slow;
                };
                match self.registers.window_mut(base + root.index(), 1) {
                    [Some(Value::Array(array))]
                        if writable_creating(array, keys.as_slice(), keys.target) =>
                    {
                        match write_creating(array, keys.as_slice(), keys.target, value) {
                            Some(old) => (true, old),
                            None => (false, None),
                        }
                    }
                    _ => (false, None),
                }
            }
            (base_path, steps)
                if steps
                    .iter()
                    .any(|step| matches!(step, PathStep::Property(_))) =>
            {
                match self.write_property_element_fast(position, base_path, steps, value) {
                    Some(old) => (true, old),
                    None => (false, None),
                }
            }
            _ => (false, None),
        };
        drop(old);
        if !written {
            return Step::Slow;
        }
        if let (Some(result), Some(result_value)) = (result, result_value) {
            let _ = store::<GUARDED, _>(self.registers.tail_mut(base), result, result_value);
        }
        Step::Done(position.ip)
    }

    /// `UnsetPath` for single-index local array `unset($arr[$k])` (including
    /// reference-wrapped arrays) without leaving `Machine::run`.
    pub(super) fn unset_element<const GUARDED: bool>(
        &mut self,
        position: Position,
        path: PathId,
    ) -> Step {
        if GUARDED {
            return Step::Slow;
        }
        let Some(path_body) = self
            .code
            .function(position.frame.function)
            .paths
            .get(path.index())
        else {
            return Step::Slow;
        };
        let (PathBase::Register(root), [PathStep::Index(key_reg)]) =
            (path_body.base, path_body.steps.as_slice())
        else {
            return Step::Slow;
        };
        let base = position.frame.base();
        let Some(key) = read_index_key(self.registers.window(base + key_reg.index(), 1)) else {
            return Step::Slow;
        };
        let (handled, removed) = match self.registers.window_mut(base + root.index(), 1) {
            [Some(Value::Array(array))] => remove_single_index(array, key),
            [Some(Value::Reference(cell))] => match cell.try_borrow_mut() {
                Ok(mut borrowed) => match &mut *borrowed {
                    Value::Array(array) => remove_single_index(array, key),
                    _ => (false, None),
                },
                Err(_) => (false, None),
            },
            _ => (false, None),
        };
        drop(removed);
        if handled {
            Step::Done(position.ip)
        } else {
            Step::Slow
        }
    }

    /// The keys of `steps`, when they are one to three `[$key]` steps with
    /// integer or string keys, the last of which may be `[]` instead.
    pub(super) fn write_keys(&self, position: Position, steps: &[PathStep]) -> Option<Keys> {
        let (steps, target) = match steps.split_last() {
            Some((PathStep::Append, levels)) => (levels, Target::Append),
            _ => (steps, Target::Last),
        };
        if (steps.is_empty() && matches!(target, Target::Last)) || steps.len() > MOST_KEYS {
            return None;
        }
        let mut keys = Keys {
            keys: [const { ArrayKey::Int(0) }; MOST_KEYS],
            count: steps.len(),
            target,
        };
        for (slot, step) in keys.keys.iter_mut().zip(steps) {
            let PathStep::Index(register) = step else {
                return None;
            };
            *slot = self.array_key(position, *register)?;
        }
        Some(keys)
    }
}

enum IndexKey {
    Int(i64),
    Interned(&'static InternedStr),
    String(PhpString),
}

#[inline(always)]
fn read_index_key(window: &[Option<Value>]) -> Option<IndexKey> {
    let [Some(value)] = window else {
        return None;
    };
    match &*value.deref() {
        Value::Int(integer) => Some(IndexKey::Int(*integer)),
        Value::InternedStr(string) => Some(IndexKey::Interned(string)),
        Value::String(string) => Some(IndexKey::String(string.clone())),
        _ => None,
    }
}

#[inline(always)]
fn write_single_index(
    array: &mut Rc<PhpArray>,
    key: IndexKey,
    value: Value,
) -> (bool, Option<Value>) {
    if Rc::strong_count(array) == 1 {
        let target = Rc::make_mut(array);
        return match key {
            IndexKey::Int(i) => match target.get_mut_int(i) {
                Some(slot) if slot.is_reference() => (false, None),
                Some(slot) => (true, Some(std::mem::replace(slot, value))),
                None => (true, target.insert_int(i, value)),
            },
            IndexKey::Interned(s) => match target.get_mut_interned(s) {
                Some(slot) if slot.is_reference() => (false, None),
                Some(slot) => (true, Some(std::mem::replace(slot, value))),
                None => (true, target.insert(s.to_array_key(), value)),
            },
            IndexKey::String(s) => match target.get_mut_str(&s) {
                Some(slot) if slot.is_reference() => (false, None),
                Some(slot) => (true, Some(std::mem::replace(slot, value))),
                None => (true, target.insert(ArrayKey::from_string(s), value)),
            },
        };
    }
    let is_ref = match &key {
        IndexKey::Int(i) => matches!(array.get_int(*i), Some(Value::Reference(_))),
        IndexKey::Interned(s) => matches!(array.get_interned(s), Some(Value::Reference(_))),
        IndexKey::String(s) => matches!(array.get_str(s), Some(Value::Reference(_))),
    };
    if is_ref {
        return (false, None);
    }
    let target = Rc::make_mut(array);
    let old = match key {
        IndexKey::Int(i) => target.insert_int(i, value),
        IndexKey::Interned(s) => match target.get_mut_interned(s) {
            Some(slot) => Some(std::mem::replace(slot, value)),
            None => target.insert(s.to_array_key(), value),
        },
        IndexKey::String(s) => match target.get_mut_str(&s) {
            Some(slot) => Some(std::mem::replace(slot, value)),
            None => target.insert(ArrayKey::from_string(s), value),
        },
    };
    (true, old)
}

#[inline(always)]
fn remove_single_index(array: &mut Rc<PhpArray>, key: IndexKey) -> (bool, Option<Value>) {
    if Rc::strong_count(array) > 1 {
        let present = match &key {
            IndexKey::Int(i) => array.contains_int(*i),
            IndexKey::Interned(s) => array.contains_interned(s),
            IndexKey::String(s) => array.contains_str(s),
        };
        if !present {
            return (true, None);
        }
    }
    let target = Rc::make_mut(array);
    let removed = match key {
        IndexKey::Int(i) => target.remove_int(i),
        IndexKey::Interned(s) => target.remove_interned(s),
        IndexKey::String(s) => target.remove_str(&s),
    };
    (true, removed)
}
