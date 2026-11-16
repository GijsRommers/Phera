//! Case-insensitive request class tables and weak object slots reproduce PHP handle reuse.

use std::{
    collections::HashMap,
    rc::{Rc, Weak},
};

use phera_value::Object;

use super::Class;

/// Only weak object owners are retained, so freed ids become available immediately.
#[derive(Default)]
pub(crate) struct ClassTable {
    pub(crate) entries: HashMap<String, Rc<Class>>,
    pub(crate) objects: Vec<Weak<Object>>,
}

impl ClassTable {
    /// Class identifiers ignore case, including names produced by dynamic expressions.
    pub(crate) fn get(&self, name: &str) -> Option<Rc<Class>> {
        self.entries.get(&name.to_ascii_lowercase()).cloned()
    }

    /// PHP reuses the lowest freed handle; zero is reserved for nonobjects.
    pub(crate) fn allocate(&mut self, class: Rc<Class>) -> Rc<Object> {
        let index = self
            .objects
            .iter()
            .position(|object| object.strong_count() == 0)
            .unwrap_or(self.objects.len());
        let object = Rc::new(Object::new(index as u32 + 1, class));
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

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use phera_extension_api::Registry;
    use phera_value::{ArrayKey, Value};

    use super::ClassTable;

    #[test]
    fn freed_object_ids_reuse_the_lowest_slot() {
        let mut table = ClassTable::native(&Registry::new());
        let class = table.get("stdClass").unwrap();
        let first = table.allocate(class.clone());
        let second = table.allocate(class.clone());
        assert_eq!((first.id, second.id), (1, 2));
        drop(first);
        let replacement = table.allocate(class);
        assert_eq!(replacement.id, 1);
        assert_eq!(Rc::strong_count(&second), 1);
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
