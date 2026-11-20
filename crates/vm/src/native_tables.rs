//! Built-in definitions shared by VMs on one thread. `new SessionHandler()` and
//! `PHP_SESSION_ACTIVE` initialize their respective tables on first lookup.

use std::{cell::OnceCell, rc::Rc, sync::Arc};

use phera_extension_api::Registry;

use crate::{classes::ClassDefinitions, constant_table::NativeConstants};

pub(crate) struct NativeClasses {
    registry: Arc<Registry>,
    definitions: OnceCell<ClassDefinitions>,
}

impl NativeClasses {
    pub(crate) fn new(registry: Arc<Registry>) -> Self {
        Self {
            registry,
            definitions: OnceCell::new(),
        }
    }

    pub(crate) fn definitions(&self) -> &ClassDefinitions {
        match self.definitions.get() {
            Some(definitions) => definitions,
            None => self.link(),
        }
    }

    #[cold]
    #[inline(never)]
    fn link(&self) -> &ClassDefinitions {
        self.definitions
            .get_or_init(|| ClassDefinitions::native(&self.registry))
    }

    pub(crate) fn initialized(&self) -> Option<&ClassDefinitions> {
        self.definitions.get()
    }
}

/// A worker shares these PHP values through `Rc`. The registry itself is process-wide.
/// The linked native definitions contain no request objects or PHP static slots.
pub struct NativeTables {
    pub(crate) registry: Arc<Registry>,
    pub(crate) classes: Rc<NativeClasses>,
    pub(crate) constants: Rc<NativeConstants>,
}

impl NativeTables {
    pub fn new(registry: Arc<Registry>) -> Self {
        Self {
            classes: Rc::new(NativeClasses::new(registry.clone())),
            constants: Rc::new(NativeConstants::new(registry.clone())),
            registry,
        }
    }
}

impl crate::Vm {
    /// A detached fiber stack starts with the same immutable definitions as its caller.
    pub(crate) fn new_fiber_stack(&self) -> Self {
        let mut child = Self::with_native_tables(&NativeTables {
            registry: self.registry.clone(),
            classes: self.classes.native_definitions(),
            constants: self.constants.native_definitions(),
        });
        child.execution_time = self.execution_time.clone();
        child
    }
}

#[cfg(test)]
mod tests {
    use std::{rc::Rc, sync::Arc};

    use phera_extension_api::Registry;
    use phera_value::Value;

    use super::NativeTables;
    use crate::Vm;

    #[test]
    fn requests_share_builtins_and_own_their_additions() {
        let mut registry = Registry::new();
        registry.constant("BUILTIN", 1);
        let tables = NativeTables::new(Arc::new(registry));
        let mut first = Vm::with_native_tables(&tables);
        let mut second = Vm::with_native_tables(&tables);
        assert!(tables.classes.initialized().is_none());
        assert!(!tables.constants.is_initialized());
        let class = first.classes.get("stdClass").unwrap();
        assert!(!tables.constants.is_initialized());
        let same_class = second.classes.get("stdClass").unwrap();
        assert!(Rc::ptr_eq(&class, &same_class));
        assert!(std::ptr::eq(
            first.constants.get("BUILTIN").unwrap(),
            second.constants.get("BUILTIN").unwrap(),
        ));

        assert!(first.classes.alias("requestalias".into(), class));
        assert!(second.classes.get("RequestAlias").is_none());
        first.set_constant("REQUEST_ONLY", Value::Int(2));
        assert!(second.constants.get("REQUEST_ONLY").is_none());
        let entry = first.classes.allocate(same_class.clone());
        assert_eq!(entry.id, second.classes.allocate(same_class).id);
    }
}
