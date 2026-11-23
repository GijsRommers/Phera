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

use phera_bytecode::{PathBase, PathId, Reg, Step as PathStep};
use phera_value::{ArrayKey, Value};

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::element_writes::{MOST_KEYS, Target, writable_creating, write_creating};
use crate::fast::store;

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
        let (written, old) = match (path_body.base, path_body.steps.as_slice()) {
            (PathBase::Register(root), [PathStep::Index(_) | PathStep::Append, ..]) => {
                let Some(keys) = self.write_keys(position, &path_body.steps) else {
                    return Step::Slow;
                };
                match self.registers.window_mut(base + root.index(), 1) {
                    [Some(Value::Array(array))]
                        if writable_creating(array, keys.as_slice(), keys.target) =>
                    {
                        match write_creating(array, keys.as_slice(), keys.target, value.clone()) {
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
                match self.write_property_element_fast(position, base_path, steps, value.clone()) {
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
        if let Some(result) = result {
            // Outside the guarded loop, into a register checked above, a
            // store completes.
            let _ = store::<false, _>(self.registers.tail_mut(base), result, value);
        }
        Step::Done(position.ip)
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
