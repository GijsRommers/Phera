//! Reads of a static property, in the interpreter loop.
//!
//! ```php
//! return static::$instance;                   // Fetch, Read
//! $alias = self::$aliases[$name] ?? null;     // Fetch, Quiet
//! if (isset(Facade::$resolved[$key])) { ... } // Fetch, Isset
//! ```
//!
//! Resolves the target class (`Post::`, `self::`, `static::`, `parent::`)
//! directly against its flat [`StaticPropTable`] indexed by case-sensitive
//! [`PropKey`], with zero per-instruction site cache, zero `RefCell` borrows,
//! and borrowed [`FastKey`] lookups for up to [`MOST_KEYS`] index steps.
//!
//! Everything else goes to the VM ([`Step::Slow`]):
//! - an undeclared class or property, a trait's own static (which deprecates),
//!   a visibility violation, or an uninitialized static slot;
//! - a missing element in a plain read, which warns;
//! - a level that is not an array, or a PHP reference on the way;
//! - a key that is not an integer or a string.

use phera_bytecode::{FetchMode, PathBase, PathId, Reg, Step as PathStep};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::element_writes::MOST_KEYS;

impl Machine<'_> {
    /// `Fetch` of a static property, or of an element of one, in `mode`.
    pub(super) fn fetch_static<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        (path, mode): (PathId, FetchMode),
    ) -> Step {
        let value = match (self.static_value(position, path), mode) {
            (Some(Some(value)), _) => value,
            (Some(None), FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty) => Value::Null,
            _ => return Step::Slow,
        };
        self.store_quiet::<GUARDED>(position, destination, value)
    }

    /// The value `path` names: `Some(None)` when a key on the way is
    /// missing, `None` for a path this does not take.
    fn static_value(&self, position: Position, path: PathId) -> Option<Option<Value>> {
        let function = position.frame.function;
        let body = self.code.function(function).paths.get(path.index())?;
        let PathBase::NamedStaticProperty { class, name, .. } = body.base else {
            return None;
        };
        if body.steps.len() > MOST_KEYS {
            return None;
        }
        let prop_key = self.code.unit_prop_key(function, name);
        let scope = self.caller_scope_index(position);
        let key = self.code.unit_symbol_key(function, class);
        let packed = if let Some(linked) = self.classes.by_key(key) {
            linked.static_prop_table().get(prop_key)?
        } else {
            let written = &self.code.unit(function).symbols.get(class.index())?.written;
            match written.as_str() {
                "self" => self
                    .caller_scope(position)?
                    .static_prop_table()
                    .get(prop_key)?,
                "parent" => self
                    .caller_scope(position)?
                    .parent
                    .as_deref()?
                    .static_prop_table()
                    .get(prop_key)?,
                "static" => self
                    .current_context()?
                    .called()?
                    .static_prop_table()
                    .get(prop_key)?,
                _ => return None,
            }
        };
        if !packed.visible_to(scope, self.classes) {
            return None;
        }
        let storage = self.class_states.existing_slot(packed.slot())?;
        if !storage.initialized.get() || !storage.value_initialized.get() {
            return None;
        }
        let cell = storage.reference.try_borrow().ok()?;
        let value = cell.try_borrow().ok()?;
        if body.steps.is_empty() {
            return Some(Some(value.deref().clone()));
        }
        let Value::Array(array) = &*value else {
            return None;
        };
        let (last, levels) = body.steps.split_last()?;
        let mut current = &**array;
        for step in levels {
            let PathStep::Index(register) = *step else {
                return None;
            };
            let key = self.array_key_ref(position, register)?;
            current = match key.get(current) {
                Some(Value::Array(inner)) => inner,
                None => return Some(None),
                Some(_) => return None,
            };
        }
        let PathStep::Index(last_register) = *last else {
            return None;
        };
        let last_key = self.array_key_ref(position, last_register)?;
        match last_key.get(current) {
            Some(Value::Reference(_)) => None,
            found => Some(found.cloned()),
        }
    }
}
