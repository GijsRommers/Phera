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

use phera_bytecode::{BinaryOp, IncDec, PathBase, PathId, Reg, Step as PathStep};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::fast_properties::Shape;
use crate::element_writes::{writable, write};
use crate::fast::store;
use crate::fast_values::quiet_binary;

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
        if GUARDED || operator == BinaryOp::Concat {
            return Step::Slow;
        }
        let right = match self
            .registers
            .window(position.frame.base() + source.index(), 1)
        {
            [Some(value)] => value.deref().clone(),
            _ => return Step::Slow,
        };
        self.update_place(position, destination, path, |left| {
            let value = quiet_binary(operator, left, &right)?;
            Some(Update {
                stored: value.clone(),
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
        if GUARDED {
            return Step::Slow;
        }
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
            (_, [PathStep::Property(_)]) => self.update_property(position, path, update),
            (_, [PathStep::Property(_), PathStep::Index(_), ..]) => {
                self.update_property_element(position, path, &path_body.steps[1..], update)
            }
            _ => None,
        };
        let Some(result) = result else {
            return Step::Slow;
        };
        // Outside the guarded loop, into a register checked above, a store
        // completes.
        let _ = store::<false>(self.registers.tail_mut(base), destination, result);
        Step::Done(position.ip)
    }

    /// `$this->property op= $value`: the expression's value.
    fn update_property(
        &self,
        position: Position,
        path: PathId,
        update: impl FnOnce(&Value) -> Option<Update>,
    ) -> Option<Value> {
        self.with_site(position, (path, Shape::Write), |entry, object, scope| {
            if entry.hooks.is_some() {
                return None;
            }
            let left = entry.read_matching(object, scope)?;
            let Update { stored, result } = update(&left)?;
            drop(left);
            entry
                .write_matching(object, scope, &stored)
                .then_some(result)
        })
    }

    /// `$local[$key]... op= $value`: the expression's value.
    fn update_local_element(
        &mut self,
        position: Position,
        root: Reg,
        steps: &[PathStep],
        update: impl FnOnce(&Value) -> Option<Update>,
    ) -> Option<Value> {
        let keys = self.write_keys(position, steps)?;
        let slot = position.frame.base() + root.index();
        let [Some(Value::Array(array))] = self.registers.window_mut(slot, 1) else {
            return None;
        };
        let Update { stored, result } = update(element(array, keys.as_slice())?)?;
        if !writable(array, keys.as_slice()) {
            return None;
        }
        let old = write(array, keys.as_slice(), stored)?;
        drop(old);
        Some(result)
    }

    /// `$this->property[$key]... op= $value`: the expression's value.
    fn update_property_element(
        &self,
        position: Position,
        path: PathId,
        steps: &[PathStep],
        update: impl FnOnce(&Value) -> Option<Update>,
    ) -> Option<Value> {
        let keys = self.write_keys(position, steps)?;
        self.with_site(position, (path, Shape::Element), |entry, object, scope| {
            if entry.hooks.is_some() {
                return None;
            }
            // The array is read and let go of before the write, so that the
            // write finds it with no second owner and need not copy it.
            let Update { stored, result } = {
                let Value::Array(array) = entry.read_matching(object, scope)? else {
                    return None;
                };
                update(element(&array, keys.as_slice())?)?
            };
            let old = entry.write_element_matching(object, scope, keys.as_slice(), stored)?;
            drop(old);
            Some(result)
        })
    }
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
