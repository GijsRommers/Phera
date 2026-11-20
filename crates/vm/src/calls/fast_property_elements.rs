//! Reads of an array element held in a property, in the interpreter loop.
//!
//! ```php
//! class Container {
//!     private array $instances = [];
//!     public function has(string $id): bool {
//!         return isset($this->instances[$id]);     // Fetch $this->instances[$id], quietly
//!     }
//!     public function get(string $id) {
//!         return $this->instances[$id];            // Fetch, reading
//!     }
//! }
//! ```
//!
//! The VM records the property's site for such a read (`Vm::prepare_element_site`)
//! as it does for a read of the property alone. When that site matches the
//! object's class and the caller's scope, and the property has no hooks,
//! the loop reads the property from it and then walks the array keys (at
//! most three, integer or string). A read gives the element when every key
//! is present; a quiet read (`??`, `isset`, `empty`) gives null when one is
//! missing or a level is null, without a diagnostic. Anything else (a missing
//! key in a read, which warns; an object with `ArrayAccess`; a string offset;
//! a PHP reference) answers [`Step::Slow`] with nothing changed.

use phera_bytecode::{FetchMode, PathId, Reg, Step as PathStep};
use phera_value::{ArrayKey, Value};

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::fast_properties::Shape;
use crate::fast::{ScalarStep, store};

/// How many keys after the property the loop walks itself.
const MOST_KEYS: usize = 3;

impl Machine<'_> {
    /// `Fetch` of `$this->property[$key]...` or `$object->property[$key]...`.
    pub(super) fn fetch_property_element<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        (path, mode): (PathId, FetchMode),
    ) -> Step {
        let quiet = match mode {
            FetchMode::Read => false,
            FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty => true,
            FetchMode::Destructure => return Step::Slow,
        };
        let Some(value) = self.property_element(position, path, quiet) else {
            return Step::Slow;
        };
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED>(slots, destination, value) {
            ScalarStep::Complete => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    pub(super) fn property_element(
        &self,
        position: Position,
        path: PathId,
        quiet: bool,
    ) -> Option<Value> {
        let property =
            self.with_site(position, (path, Shape::Element), |entry, object, scope| {
                if entry.hooks.is_some() {
                    return None;
                }
                entry.read_matching(object, scope)
            })?;
        let keys = &self
            .code
            .function(position.frame.function)
            .paths
            .get(path.index())?
            .steps[1..];
        if keys.len() > MOST_KEYS {
            return None;
        }
        let mut current = property;
        for step in keys {
            let PathStep::Index(register) = step else {
                return None;
            };
            let key = self.array_key(position, *register)?;
            current = match &current {
                Value::Array(array) => match array.get(&key) {
                    Some(Value::Reference(_)) => return None,
                    Some(element) => element.clone(),
                    None if quiet => return Some(Value::Null),
                    None => return None,
                },
                Value::Null if quiet => return Some(Value::Null),
                _ => return None,
            };
        }
        Some(current)
    }

    /// An integer or string key in a register, as PHP converts it.
    pub(super) fn array_key(&self, position: Position, register: Reg) -> Option<ArrayKey> {
        match self
            .registers
            .window(position.frame.base() + register.index(), 1)
        {
            [Some(Value::Int(integer))] => Some(ArrayKey::Int(*integer)),
            [Some(Value::String(string))] => Some(ArrayKey::from_string(string.clone())),
            _ => None,
        }
    }
}
