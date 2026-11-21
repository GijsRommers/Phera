//! Built-in definitions shared by VMs on one thread. `new SessionHandler()` and
//! `PHP_SESSION_ACTIVE` resolve against metadata linked when the worker starts.

use std::{rc::Rc, sync::Arc};

use phera_extension_api::Registry;

use crate::{
    classes::ClassDefinitions,
    constant_table::{NativeConstants, native_constants},
};

/// A worker shares these PHP values through `Rc`. The registry itself is process-wide.
/// The linked native definitions contain no request objects or PHP static slots.
pub struct NativeTables {
    pub(crate) registry: Arc<Registry>,
    pub(crate) classes: Rc<ClassDefinitions>,
    pub(crate) constants: Rc<NativeConstants>,
    /// The worker's function numbers (see `crate::worker_code`).
    pub(crate) code: Rc<crate::worker_code::WorkerCode>,
    /// The worker's static and constant slot numbers.
    pub(crate) slots: Rc<crate::classes::class_state::SlotNumbers>,
    /// The worker's linked classes (see `crate::classes::worker_classes`).
    pub(crate) linked: Rc<crate::classes::worker_classes::WorkerClasses>,
}

impl NativeTables {
    pub fn new(registry: Arc<Registry>) -> Self {
        Self {
            classes: Rc::new({
                let mut classes = ClassDefinitions::native(&registry);
                classes.number_keys();
                classes
            }),
            constants: Rc::new(native_constants(&registry)),
            code: Rc::default(),
            slots: Rc::default(),
            linked: Rc::default(),
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
            code: self.code.worker(),
            slots: self.class_states.numbers(),
            linked: self.classes.worker(),
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
        let class = first.classes.get("stdClass").unwrap();
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
