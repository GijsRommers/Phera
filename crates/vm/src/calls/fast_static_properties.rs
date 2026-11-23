//! Reads of a static property, in the interpreter loop.
//!
//! ```php
//! return static::$instance;                   // Fetch, Read
//! $alias = self::$aliases[$name] ?? null;     // Fetch, Quiet
//! if (isset(Facade::$resolved[$key])) { ... } // Fetch, Isset
//! ```
//!
//! The path names its class (`Post::`, `self::`, `static::`, `parent::`) and
//! has up to [`MOST_KEYS`] index steps. The VM recorded what the property's
//! first read found in the instruction's site (see `crate::static_sites`):
//! the slot of its value and the class context that holds for. When the
//! running code has that context, the value is read from the slot here, as
//! the VM's `Fetch` gives it: the value, or for `isset`, `??` and `empty`
//! null when a key is missing.
//!
//! Everything else goes to the VM ([`Step::Slow`]):
//! - a site without an entry, or made for another class scope or called
//!   class;
//! - a missing element in a plain read, which warns;
//! - a level that is not an array, or a PHP reference on the way;
//! - a key that is not an integer or a string.

use phera_bytecode::{FetchMode, PathBase, PathId, Reg, Step as PathStep};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::element_writes::{MOST_KEYS, Target};
use crate::unit_caches::CachedSite;

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
    /// missing, `None` for a path or a site this does not take.
    fn static_value(&self, position: Position, path: PathId) -> Option<Option<Value>> {
        let function = position.frame.function;
        let body = self.code.function(function).paths.get(path.index())?;
        if !matches!(body.base, PathBase::NamedStaticProperty { .. })
            || body.steps.len() > MOST_KEYS
            || body
                .steps
                .iter()
                .any(|step| !matches!(step, PathStep::Index(_)))
        {
            return None;
        }
        // The VM keys the site by the position after the instruction,
        // which is where the loop stands now.
        let site = self.code.site(function, position.ip)?.try_borrow().ok()?;
        let CachedSite::Static(entry) = &*site else {
            return None;
        };
        let context = self.current_context();
        if context.as_ref().and_then(|context| context.scope_index()) != entry.scope
            || (entry.called.is_some()
                && context.as_ref().and_then(|context| context.called_index()) != entry.called)
        {
            return None;
        }
        let storage = self.class_states.existing_slot(entry.slot)?;
        if !storage.initialized.get() || !storage.value_initialized.get() {
            return None;
        }
        let cell = storage.reference.try_borrow().ok()?;
        let value = cell.try_borrow().ok()?;
        if body.steps.is_empty() {
            return Some(Some(value.deref().clone()));
        }
        let keys = self.write_keys(position, &body.steps)?;
        if !matches!(keys.target, Target::Last) {
            return None;
        }
        let Value::Array(array) = &*value else {
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
        match current.get(last) {
            Some(Value::Reference(_)) => None,
            found => Some(found.cloned()),
        }
    }
}
