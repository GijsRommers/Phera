//! Compound assignments of a property or an array element, in the
//! interpreter loop.
//!
//! ```php
//! $this->count += $n;          // AssignOpPath of a declared property
//! $totals[$key] -= $amount;    // of an element of a local array
//! $this->sums[$key] *= 2;      // of an element of an array property
//! $counts[$word]++;            // IncDecPath of an element
//! ```
//!
//! The loop reads the place as a read of it does (the property's site,
//! `fast_properties`; the keys, `fast_property_elements`), applies the
//! operator when both operands are plain values and it raises nothing
//! (`fast_values::quiet_binary`; `++` and `--` of an integer or a float),
//! and writes the result back as a plain
//! write does (`PropertyEntry::write_matching`, `crate::element_writes`).
//! The VM records the property's site when it makes such an update itself.
//!
//! The VM keeps everything else: a missing element or property (which
//! warns), hooks, magic methods, a result that does not fit the
//! property's type, a PHP reference on the way, objects with their own
//! operators, `.=` (which the VM can append to in place), and the guarded
//! loop, where the old value let go of could leave a destructor due.

use phera_bytecode::{BinaryOp, IncDec, NameId, PathBase, PathId, Reg, Step as PathStep};
use phera_value::{PhpArray, PhpString, Value};
use std::rc::Rc;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::fast_properties::write_slot_direct;
use crate::element_writes::{Target, writable, write};
use crate::fast::{duplicate, store};
use crate::fast_values::quiet_binary;
use crate::operators::fast_binary;

/// What an update stores in the place and what the expression gives:
/// the same value for `+=`, the old one for `$x++`.
struct Update {
    stored: Value,
    result: Value,
}

impl Machine<'_> {
    /// `AssignOpPath` of a declared property or of an array element.
    pub(super) fn assign_operation<const GUARDED: bool>(
        &mut self,
        position: Position,
        (operator, destination): (BinaryOp, Reg),
        path: PathId,
        source: Reg,
    ) -> Step {
        if operator == BinaryOp::Concat {
            return Step::Slow;
        }
        let right = match self
            .registers
            .window(position.frame.base() + source.index(), 1)
        {
            [Some(value)] => duplicate(&value.deref()),
            _ => return Step::Slow,
        };
        self.update_place(position, destination, path, |left| {
            let value = fast_binary(operator, left, &right)
                .or_else(|| quiet_binary(operator, left, &right))?;
            Some(Update {
                stored: duplicate(&value),
                result: value,
            })
        })
    }

    /// `IncDecPath` of an array element holding an integer or a float.
    /// (`++` of a property is `fast_properties::increment_property`.)
    pub(super) fn increment_element<const GUARDED: bool>(
        &mut self,
        position: Position,
        (mode, destination): (IncDec, Reg),
        path: PathId,
    ) -> Step {
        let up = matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement);
        let post = matches!(mode, IncDec::PostIncrement | IncDec::PostDecrement);
        self.update_place(position, destination, path, |old| {
            let new = match old {
                Value::Int(old) => Value::Int(if up {
                    old.checked_add(1)?
                } else {
                    old.checked_sub(1)?
                }),
                Value::Float(old) => Value::Float(if up { old + 1.0 } else { old - 1.0 }),
                _ => return None,
            };
            Some(Update {
                result: if post { old.clone() } else { new.clone() },
                stored: new,
            })
        })
    }

    /// Applies `update` to the place `path` names and stores the
    /// expression's value in `destination`, or answers [`Step::Slow`] with
    /// nothing changed.
    fn update_place(
        &mut self,
        position: Position,
        destination: Reg,
        path: PathId,
        update: impl FnOnce(&Value) -> Option<Update>,
    ) -> Step {
        let base = position.frame.base();
        // Once the place has changed, the result must be stored.
        if self
            .registers
            .window(base + destination.index(), 1)
            .is_empty()
        {
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
        let result = match (path_body.base, path_body.steps.as_slice()) {
            (PathBase::Register(root), [PathStep::Index(_), ..]) => {
                self.update_local_element(position, root, &path_body.steps, update)
            }
            (path_base, [PathStep::Property(name)]) => {
                self.update_property(position, path_base, *name, update)
            }
            (path_base, [PathStep::Property(name), rest @ ..])
                if rest.iter().all(|step| matches!(step, PathStep::Index(_))) =>
            {
                self.update_property_element(position, path_base, *name, rest, update)
            }
            _ => None,
        };
        let Some(result) = result else {
            return Step::Slow;
        };
        // Outside the guarded loop, into a register checked above, a store
        // completes.
        let _ = store::<false, _>(self.registers.tail_mut(base), destination, result);
        Step::Done(position.ip)
    }

    /// `$this->property op= $value`: direct flat [`PropertyTable`] read & write
    /// with zero per-instruction site cache or `RefCell` borrow.
    fn update_property(
        &self,
        position: Position,
        path_base: PathBase,
        name: NameId,
        update: impl FnOnce(&Value) -> Option<Update>,
    ) -> Option<Value> {
        let object = self.property_object(position, path_base)?;
        let scope = self.caller_scope_index(position);
        let (table, packed) = self.resolve_prop(position, object, name, scope)?;
        if !packed.is_plain_writable() {
            return None;
        }
        let slot = packed.slot();
        let left = object
            .with_properties(|properties| match properties.slot(slot)? {
                Value::Reference(_) => None,
                value => Some(value.clone()),
            })
            .flatten()?;
        let Update { stored, result } = update(&left)?;
        drop(left);
        if !packed.allows_type(crate::types::own_type(&stored)) {
            return None;
        }
        let mut stored = Some(stored);
        write_slot_direct(object, slot, &mut stored, table.fills_missing_directly())
            .then_some(result)
    }

    /// `$local[$key]... op= $value`: the expression's value.
    fn update_local_element(
        &mut self,
        position: Position,
        root: Reg,
        steps: &[PathStep],
        update: impl FnOnce(&Value) -> Option<Update>,
    ) -> Option<Value> {
        // Fast path for single-index `$arr[$k] op= $v`: one lookup, no
        // `ArrayKey` clone, no redundant `writable()` re-walk, and supports
        // reference-wrapped arrays (`&$arr`).
        if let [PathStep::Index(key_reg)] = steps {
            let key = read_fast_key(
                self.registers
                    .window(position.frame.base() + key_reg.index(), 1),
            )?;
            let slot = position.frame.base() + root.index();
            return match self.registers.window_mut(slot, 1) {
                [Some(Value::Array(array))] => update_single_array(array, key, update),
                [Some(Value::Reference(cell))] => {
                    let mut borrowed = cell.try_borrow_mut().ok()?;
                    match &mut *borrowed {
                        Value::Array(array) => update_single_array(array, key, update),
                        _ => None,
                    }
                }
                _ => None,
            };
        }
        let keys = self.write_keys(position, steps)?;
        let slot = position.frame.base() + root.index();
        let [Some(Value::Array(array))] = self.registers.window_mut(slot, 1) else {
            return None;
        };
        if matches!(keys.target, Target::Append) {
            return None;
        }
        let Update { stored, result } = update(element(array, keys.as_slice())?)?;
        if !writable(array, keys.as_slice(), Target::Last) {
            return None;
        }
        let old = write(array, keys.as_slice(), Target::Last, stored)?;
        drop(old);
        Some(result)
    }

    /// `$this->property[$key]... op= $value`: direct flat [`PropertyTable`]
    /// single-borrow update with zero array clone or site cache.
    fn update_property_element(
        &self,
        position: Position,
        path_base: PathBase,
        name: NameId,
        steps: &[PathStep],
        update: impl FnOnce(&Value) -> Option<Update>,
    ) -> Option<Value> {
        let keys = self
            .write_keys(position, steps)
            .filter(|keys| matches!(keys.target, Target::Last))?;
        let object = self.property_object(position, path_base)?;
        let scope = self.caller_scope_index(position);
        let (_, packed) = self.resolve_prop(position, object, name, scope)?;
        if !packed.is_plain_writable() || !packed.allows_array() {
            return None;
        }
        let slot = packed.slot();
        let (result, old) = object
            .update(|properties| {
                properties.layout()?;
                let Some(Value::Array(array)) = properties.slot_mut(slot) else {
                    return None;
                };
                if !writable(array, keys.as_slice(), Target::Last) {
                    return None;
                }
                let Update { stored, result } = update(element(array, keys.as_slice())?)?;
                let old = write(array, keys.as_slice(), Target::Last, stored)?;
                Some((result, old))
            })
            .flatten()?;
        drop(old);
        Some(result)
    }
}

enum FastKey {
    Int(i64),
    Interned(&'static phera_value::InternedStr),
    String(PhpString),
}

#[inline(always)]
fn read_fast_key(window: &[Option<Value>]) -> Option<FastKey> {
    let [Some(value)] = window else {
        return None;
    };
    match &*value.deref() {
        Value::Int(integer) => Some(FastKey::Int(*integer)),
        Value::InternedStr(string) => Some(FastKey::Interned(string)),
        Value::String(string) => Some(FastKey::String(string.clone())),
        _ => None,
    }
}

#[inline(always)]
fn update_single_array(
    array: &mut Rc<PhpArray>,
    key: FastKey,
    update: impl FnOnce(&Value) -> Option<Update>,
) -> Option<Value> {
    if Rc::strong_count(array) == 1 {
        let target = Rc::make_mut(array);
        let slot = match &key {
            FastKey::Int(i) => target.get_mut_int(*i)?,
            FastKey::Interned(s) => target.get_mut_interned(s)?,
            FastKey::String(s) => target.get_mut_str(s)?,
        };
        if slot.is_reference() {
            return None;
        }
        let Update { stored, result } = update(slot)?;
        let old = std::mem::replace(slot, stored);
        drop(old);
        return Some(result);
    }
    let existing = match &key {
        FastKey::Int(i) => array.get_int(*i)?,
        FastKey::Interned(s) => array.get_interned(s)?,
        FastKey::String(s) => array.get_str(s)?,
    };
    if existing.is_reference() {
        return None;
    }
    let Update { stored, result } = update(existing)?;
    let target = Rc::make_mut(array);
    let slot = match &key {
        FastKey::Int(i) => target.get_mut_int(*i)?,
        FastKey::Interned(s) => target.get_mut_interned(s)?,
        FastKey::String(s) => target.get_mut_str(s)?,
    };
    let old = std::mem::replace(slot, stored);
    drop(old);
    Some(result)
}

/// The element at `keys`, when every level is an array, the element
/// exists and is not a PHP reference.
fn element<'a>(
    array: &'a phera_value::PhpArray,
    keys: &[phera_value::ArrayKey],
) -> Option<&'a Value> {
    let (last, levels) = keys.split_last()?;
    let mut current = array;
    for key in levels {
        current = match current.get(key)? {
            Value::Array(inner) => inner,
            _ => return None,
        };
    }
    match current.get(last)? {
        Value::Reference(_) => None,
        value => Some(value),
    }
}
