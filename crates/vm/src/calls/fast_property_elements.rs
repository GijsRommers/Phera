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
    #[inline(never)]
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
        match store::<GUARDED, _>(slots, destination, value) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    pub(super) fn property_element(
        &self,
        position: Position,
        path: PathId,
        quiet: bool,
    ) -> Option<Value> {
        let [PathStep::Property(_), keys @ ..] = self
            .code
            .function(position.frame.function)
            .paths
            .get(path.index())?
            .steps
            .as_slice()
        else {
            return None;
        };
        if keys.is_empty() || keys.len() > MOST_KEYS {
            return None;
        }
        let base = position.frame.base();
        let mut key_values: [Option<&Value>; MOST_KEYS] = [None; MOST_KEYS];
        for (slot, step) in key_values.iter_mut().zip(keys) {
            let PathStep::Index(register) = step else {
                return None;
            };
            let [Some(key @ (Value::Int(_) | Value::InternedStr(_) | Value::String(_)))] =
                self.registers.window(base + register.index(), 1)
            else {
                return None;
            };
            *slot = Some(key);
        }
        let key_slice = &key_values[..keys.len()];
        self.with_site(position, (path, Shape::Element), |entry, object, scope| {
            entry.with_matching_value(object, scope, |mut current| {
                for key in key_slice {
                    current = match current {
                        Value::Array(array) => {
                            let found = match key.expect("validated above") {
                                Value::Int(integer) => array.get_int(*integer),
                                Value::InternedStr(string) => array.get_interned(string),
                                Value::String(string) => array.get_str(string),
                                _ => unreachable!("validated above"),
                            };
                            match found {
                                Some(Value::Reference(_)) => return None,
                                Some(element) => element,
                                None if quiet => return Some(Value::Null),
                                None => return None,
                            }
                        }
                        Value::Null if quiet => return Some(Value::Null),
                        _ => return None,
                    };
                }
                Some(current.clone())
            })
        })
    }

    /// An integer or string key in a register, as PHP converts it.
    pub(super) fn array_key(&self, position: Position, register: Reg) -> Option<ArrayKey> {
        match self
            .registers
            .window(position.frame.base() + register.index(), 1)
        {
            [Some(Value::Int(integer))] => Some(ArrayKey::Int(*integer)),
            [Some(Value::InternedStr(string))] => Some(string.to_array_key()),
            [Some(Value::String(string))] => Some(ArrayKey::from_string(string.clone())),
            _ => None,
        }
    }
}
