//! Compound assignments of a property or an array element, in the
//! interpreter loop.
//!
//! ```php
//! $this->count += $n;          // AssignOpPath of a declared property
//! $totals[$key] -= $amount;    // of an element of a local array
//! $this->sums[$key] *= 2;      // of an element of an array property
//! ```
//!
//! The loop reads the place as a read of it does (the property's site,
//! `fast_properties`; the keys, `fast_property_elements`), applies the
//! operator when both operands are plain values and it raises nothing
//! (`fast_values::quiet_binary`), and writes the result back as a plain
//! write does (`PropertyEntry::write_matching`, `crate::element_writes`).
//! The VM records the property's site when it makes such an update itself.
//!
//! The VM keeps everything else: a missing element or property (which
//! warns), hooks, magic methods, a result that does not fit the
//! property's type, a PHP reference on the way, objects with their own
//! operators, `.=` (which the VM can append to in place), and the guarded
//! loop, where the old value let go of could leave a destructor due.

use phera_bytecode::{BinaryOp, PathBase, PathId, Reg, Step as PathStep};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::fast_properties::Shape;
use crate::element_writes::{writable, write};
use crate::fast::store;
use crate::fast_values::quiet_binary;

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
        let base = position.frame.base();
        // Once the place has changed, the result must be stored.
        if self
            .registers
            .window(base + destination.index(), 1)
            .is_empty()
        {
            return Step::Slow;
        }
        let right = match self.registers.window(base + source.index(), 1) {
            [Some(value)] => value.deref().clone(),
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
        let result = match (path_body.base, path_body.steps.as_slice()) {
            (PathBase::Register(root), [PathStep::Index(_), ..]) => {
                self.update_local_element(position, root, &path_body.steps, (operator, &right))
            }
            (_, [PathStep::Property(_)]) => {
                self.update_property(position, path, (operator, &right))
            }
            (_, [PathStep::Property(_), PathStep::Index(_), ..]) => self.update_property_element(
                position,
                path,
                &path_body.steps[1..],
                (operator, &right),
            ),
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

    /// `$this->property op= $value`: the result, written back.
    fn update_property(
        &self,
        position: Position,
        path: PathId,
        (operator, right): (BinaryOp, &Value),
    ) -> Option<Value> {
        self.with_site(position, (path, Shape::Write), |entry, object, scope| {
            if entry.hooks.is_some() {
                return None;
            }
            let left = entry.read_matching(object, scope)?;
            let result = quiet_binary(operator, &left, right)?;
            drop(left);
            entry
                .write_matching(object, scope, &result)
                .then_some(result)
        })
    }

    /// `$local[$key]... op= $value`: the result, written back.
    fn update_local_element(
        &mut self,
        position: Position,
        root: Reg,
        steps: &[PathStep],
        (operator, right): (BinaryOp, &Value),
    ) -> Option<Value> {
        let keys = self.write_keys(position, steps)?;
        let slot = position.frame.base() + root.index();
        let [Some(Value::Array(array))] = self.registers.window_mut(slot, 1) else {
            return None;
        };
        let left = element(array, keys.as_slice())?;
        let result = quiet_binary(operator, left, right)?;
        if !writable(array, keys.as_slice()) {
            return None;
        }
        let old = write(array, keys.as_slice(), result.clone())?;
        drop(old);
        Some(result)
    }

    /// `$this->property[$key]... op= $value`: the result, written back.
    fn update_property_element(
        &self,
        position: Position,
        path: PathId,
        steps: &[PathStep],
        (operator, right): (BinaryOp, &Value),
    ) -> Option<Value> {
        let keys = self.write_keys(position, steps)?;
        self.with_site(position, (path, Shape::Element), |entry, object, scope| {
            if entry.hooks.is_some() {
                return None;
            }
            // The array is read and let go of before the write, so that the
            // write finds it with no second owner and need not copy it.
            let result = {
                let Value::Array(array) = entry.read_matching(object, scope)? else {
                    return None;
                };
                quiet_binary(operator, element(&array, keys.as_slice())?, right)?
            };
            let old =
                entry.write_element_matching(object, scope, keys.as_slice(), result.clone())?;
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
