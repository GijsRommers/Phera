//! Reads of an element of a local array, in the interpreter loop.
//!
//! ```php
//! $type = $row['type'];                   // Fetch, Read
//! if (isset($data[$field])) { ... }       // Fetch, Isset
//! $name = $options['name'] ?? 'default';  // Fetch, Quiet
//! if (empty($cache[$group][$key])) { ... } // Fetch, Empty
//! ```
//!
//! The path starts at a register that holds an array and walks up to
//! [`MOST_KEYS`] integer or string keys through arrays. A read gives the
//! element's value. `isset`, `??` and `empty` give the value too, or null
//! when a key is missing, and the instruction after them tests it, as the
//! VM's `Fetch` does.
//!
//! Everything else goes to the VM ([`Step::Slow`]):
//! - a missing element in a plain read, which warns;
//! - a key that is not an integer or a string;
//! - a level that is not an array (a string offset, an `ArrayAccess`
//!   object, null);
//! - a register that holds a PHP reference;
//! - a function with a dynamic scope (`$$name`, `extract`).

use phera_bytecode::{FetchMode, PathBase, PathId, Reg, Step as PathStep};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::element_writes::{MOST_KEYS, Target};

impl Machine<'_> {
    /// `Fetch` of an element of a local array, in `mode`.
    pub(super) fn fetch_local_element<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        (path, mode): (PathId, FetchMode),
    ) -> Step {
        let value = match (self.local_element(position, path), mode) {
            (Some(Some(element)), _) => element.deref().clone(),
            (Some(None), FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty) => Value::Null,
            _ => return Step::Slow,
        };
        self.store_quiet::<GUARDED>(position, destination, value)
    }

    /// The element `path` names in a local array: `Some(None)` when a key
    /// on the way is missing, `None` for a path or a value that is not
    /// this shape. The element may be a PHP reference.
    pub(super) fn local_element(&self, position: Position, path: PathId) -> Option<Option<&Value>> {
        let function = self.code.function(position.frame.function);
        if function.dynamic_scope {
            return None;
        }
        let body = function.paths.get(path.index())?;
        let PathBase::Register(root) = body.base else {
            return None;
        };
        if !matches!(body.steps.first(), Some(PathStep::Index(_))) || body.steps.len() > MOST_KEYS {
            return None;
        }
        let keys = self.write_keys(position, &body.steps)?;
        if !matches!(keys.target, Target::Last) {
            return None;
        }
        let [Some(Value::Array(array))] = self
            .registers
            .window(position.frame.base() + root.index(), 1)
        else {
            return None;
        };
        let (last, levels) = keys.as_slice().split_last()?;
        let mut current = &**array;
        for key in levels {
            current = match current.get(key) {
                Some(Value::Array(inner)) => inner,
                None => return Some(None),
                Some(_) => return None,
            };
        }
        Some(current.get(last))
    }
}
