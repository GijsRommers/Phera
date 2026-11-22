//! What a class's property accesses may involve, in one word made when the
//! class is linked.
//!
//! ```php
//! #[AllowDynamicProperties] class Row { public int $id; }   // DYNAMIC
//! class Lazy { public function __get($name) { ... } }       // MAGIC_GET
//! final class Point { public readonly int $x; }             // READONLY
//! ```
//!
//! A property access reads this word first. When none of the flags that
//! matter to it is set, there is nothing to plan: the access is a slot of a
//! declared property or a key of the undeclared ones, done at once and
//! without PHP code running. Only a flag that is set sends it to the part
//! of the general path that can run PHP code (a magic method, a hook) or
//! raise what PHP raises (readonly, a deprecation).

use phera_value::ClassHandle;

use super::class::Class;

/// The flags; see the module.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PropertyFacts(u16);

impl PropertyFacts {
    pub(crate) const MAGIC_GET: Self = Self(1);
    pub(crate) const MAGIC_SET: Self = Self(1 << 1);
    pub(crate) const MAGIC_ISSET: Self = Self(1 << 2);
    pub(crate) const MAGIC_UNSET: Self = Self(1 << 3);
    /// Some declared property has hooks (or the class's native code
    /// supplies its properties).
    pub(crate) const HOOKS: Self = Self(1 << 4);
    /// Some declared property is readonly, or the class is.
    pub(crate) const READONLY: Self = Self(1 << 5);
    /// Properties may be created that the class does not declare, without
    /// a deprecation (`stdClass`, `#[AllowDynamicProperties]`).
    pub(crate) const DYNAMIC: Self = Self(1 << 6);

    /// The facts of `class`, from what linking made of it.
    pub(crate) fn of(class: &Class) -> Self {
        let mut facts = Self::default();
        if let Some(magic) = class.magic.as_ref() {
            facts.set(Self::MAGIC_GET, magic.get.is_some());
            facts.set(Self::MAGIC_SET, magic.set.is_some());
            facts.set(Self::MAGIC_ISSET, magic.isset.is_some());
            facts.set(Self::MAGIC_UNSET, magic.unset.is_some());
        }
        facts.set(
            Self::HOOKS,
            class.has_native_properties()
                || class.property_access.is_some()
                || class
                    .defaults
                    .iter()
                    .any(|property| property.hooks.is_some()),
        );
        facts.set(
            Self::READONLY,
            class.is_readonly || class.defaults.iter().any(|property| property.is_readonly),
        );
        facts.set(
            Self::DYNAMIC,
            class.allows_dynamic_properties && !class.is_readonly,
        );
        facts
    }

    fn set(&mut self, flag: Self, on: bool) {
        if on {
            self.0 |= flag.0;
        }
    }

    pub(crate) fn has(self, flag: Self) -> bool {
        self.0 & flag.0 != 0
    }
}
