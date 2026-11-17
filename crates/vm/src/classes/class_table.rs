//! Request class lookup, leased object ids, and weak cycle-cleanup records.

use std::{
    collections::HashMap,
    rc::{Rc, Weak},
};

use phera_value::{Object, ObjectIds};

use super::Class;

/// One request owns the id pool; weak records exist solely for cycle cleanup.
#[derive(Default)]
pub(crate) struct ClassTable {
    /// Preserve registration/declaration order for get_declared_classes.
    pub(crate) order: Vec<String>,
    pub(crate) entries: HashMap<String, Rc<Class>>,
    pub(crate) objects: Vec<Weak<Object>>,
    ids: Rc<ObjectIds>,
}

impl ClassTable {
    /// Class identifiers ignore case, including names produced by dynamic expressions.
    pub(crate) fn get(&self, name: &str) -> Option<Rc<Class>> {
        if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
            self.entries.get(&name.to_ascii_lowercase()).cloned()
        } else {
            self.entries.get(name).cloned()
        }
    }

    /// Value-owned leases implement PHP's LIFO reuse without inspecting weak handles.
    pub(crate) fn allocate(&mut self, class: Rc<Class>) -> Rc<Object> {
        let object = Rc::new(Object::allocate(&self.ids, class));
        // Record the already assigned id only for request-end cycle cleanup.
        // No weak handle's liveness participates in allocation or id selection.
        let index = object.id.saturating_sub(1) as usize;
        if let Some(slot) = self.objects.get_mut(index) {
            *slot = Rc::downgrade(&object);
        } else {
            self.objects.push(Rc::downgrade(&object));
        }
        object
    }
}

impl Drop for ClassTable {
    /// Break request-local object cycles without retaining objects during normal execution.
    fn drop(&mut self) {
        for object in self.objects.iter().filter_map(Weak::upgrade) {
            object.update(|properties| *properties = phera_value::PhpArray::new());
        }
    }
}

impl ClassTable {
    /// Mirrors name lookup for external handles, while linked VM classes carry
    /// a weak self identity whose owner is already kept alive by the object.
    pub(crate) fn for_object(&self, object: &Object) -> Option<Rc<Class>> {
        object
            .class
            .as_any()
            .downcast_ref::<Class>()
            .and_then(|class| class.identity.upgrade())
            .or_else(|| self.get(object.class.name()))
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use phera_extension_api::Registry;
    use phera_value::{ArrayKey, Value};

    use super::ClassTable;

    #[test]
    fn freed_object_ids_reuse_the_most_recently_freed_slot() {
        let mut table = ClassTable::native(&Registry::new());
        let class = table.get("stdClass").unwrap();
        let first = table.allocate(class.clone());
        let second = table.allocate(class.clone());
        assert_eq!((first.id, second.id), (1, 2));
        drop(first);
        let replacement = table.allocate(class);
        assert_eq!(replacement.id, 1);
        assert_eq!(Rc::strong_count(&second), 1);
        let third = table.allocate(table.get("stdClass").unwrap());
        assert_eq!(third.id, 3);
        drop(replacement);
        drop(second);
        let class = table.get("stdClass").unwrap();
        let reused_second = table.allocate(class.clone());
        let reused_first = table.allocate(class.clone());
        let next = table.allocate(class);
        assert_eq!((reused_second.id, reused_first.id, next.id), (2, 1, 4));
    }

    #[test]
    fn a_million_linked_objects_drop_without_rust_recursion() {
        let table = ClassTable::native(&Registry::new());
        let class = table.get("stdClass").unwrap();
        let mut head = Value::Null;
        for index in 0..1_000_000 {
            let object = Rc::new(phera_value::Object::new(index + 1, class.clone()));
            object.set(ArrayKey::str("next"), head);
            head = Value::Object(object);
        }
        drop(head);
    }
}
