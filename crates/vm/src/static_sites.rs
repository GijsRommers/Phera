//! A static property read the interpreter loop makes without the VM.
//!
//! ```php
//! return static::$instance;                // static::, keyed by the called class
//! $alias = self::$aliases[$name] ?? null;  // an element, quietly
//! if (isset(Facade::$resolved[$key])) { ... }
//! ```
//!
//! The first read of a static property by a named class (`Post::`, `self::`,
//! `static::`, `parent::`) runs in the VM, which raises PHP's errors and
//! evaluates the default. When that read succeeds it records what it found
//! in the instruction's site: the static's slot in the request's class
//! states, and the class context the answer holds for. The loop then reads
//! the slot itself while the running code has that context (see
//! `calls::fast_static_properties`).
//!
//! An entry is three numbers, no pointers. Slots are numbered by the worker,
//! so an entry made before a snapshot means the same in every request forked
//! from it.

use phera_bytecode::{PathBase, Step};
use phera_extension_api::NativeError;
use std::rc::Rc;

use crate::classes::ClassIndex;
use crate::classes::class_state::StaticSlot;
use crate::unit_caches::CachedSite;
use crate::{AccessPath, Vm};

/// What a static property read site found.
pub(crate) struct StaticEntry {
    /// The class scope of the code that read it: the class `self::` names,
    /// and the scope its visibility was checked from.
    pub(crate) scope: Option<ClassIndex>,
    /// For `static::`, the called class it was resolved for; `None` for a
    /// class the source names, `self::` and `parent::`, which the scope
    /// fixes.
    pub(crate) called: Option<ClassIndex>,
    /// Where the request keeps the property's value.
    pub(crate) slot: StaticSlot,
}

impl Vm {
    /// Records the site of a static property read that just succeeded, so
    /// that the loop makes the next one itself. The path is a named class's
    /// static property with up to the loop's number of index steps; a trait's
    /// own static (deprecated access), a property not visible from here or
    /// not initialized gets no entry.
    #[cold]
    #[inline(never)]
    pub(crate) fn prepare_static_site(&mut self, place: &AccessPath) -> Result<(), NativeError> {
        let PathBase::NamedStaticProperty { class, name, .. } = place.base()? else {
            return Ok(());
        };
        if place.steps()?.any(|step| !matches!(step, Step::Index(_))) {
            return Ok(());
        }
        let Some(frame) = self.current() else {
            return Ok(());
        };
        let function = frame.frame.function;
        let relative = self.relative_class(class);
        let found = match &relative {
            Some(class) => class.clone(),
            None => self
                .classes
                .by_key(self.code.unit_symbol_key(function, class))
                .cloned(),
        };
        let Some(found) = found else {
            return Ok(());
        };
        let name = &self.code.unit(function).names[name.index()];
        let Some(property) = found
            .static_properties
            .iter()
            .rev()
            .find(|property| property.name == *name)
        else {
            return Ok(());
        };
        if found.kind == phera_bytecode::ClassKind::Trait || !self.static_visible(property) {
            return Ok(());
        }
        let (Some(slot), Some(storage)) = (
            property.static_storage,
            self.class_states.existing_static(property),
        ) else {
            return Ok(());
        };
        if !storage.initialized.get() || !storage.value_initialized.get() {
            return Ok(());
        }
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("static site"))?;
        let is_static = frame.unit().symbols[class.index()].written == "static";
        let entry = StaticEntry {
            scope: frame.scope().map(|scope| scope.index),
            called: if is_static {
                Some(found.index)
            } else {
                None
            },
            slot,
        };
        if let Some(site) = frame.site_cache()
            && let Ok(mut site) = site.try_borrow_mut()
        {
            *site = CachedSite::Static(Rc::new(entry));
        }
        Ok(())
    }
}
