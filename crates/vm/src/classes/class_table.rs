//! Request class lookup, leased object ids, and weak cycle-cleanup records.

use std::{
    collections::HashMap,
    rc::{Rc, Weak},
};

use phera_value::{Object, ObjectIds};

use super::Class;

/// A class's position in the request's class table: a method frame's class
/// scope and called class are kept as these numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ClassIndex(u32);

impl ClassIndex {
    pub(crate) fn number(self) -> u32 {
        self.0
    }
}

/// One request owns the id pool; weak records exist solely for cycle cleanup.
#[derive(Default)]
pub(crate) struct ClassTable {
    /// Preserve registration/declaration order for get_declared_classes.
    pub(crate) order: Vec<String>,
    pub(crate) entries: HashMap<String, Rc<Class>>,
    /// Every class, by number.
    numbered: Vec<Rc<Class>>,
    closure: Option<ClassIndex>,
    pub(crate) has_dimensions: bool,
    pub(crate) has_property_views: bool,
    pub(crate) objects: Vec<Weak<Object>>,
    ids: Rc<ObjectIds>,
}

impl ClassTable {
    /// The number the next registered class gets.
    pub(crate) fn next_index(&self) -> ClassIndex {
        ClassIndex(self.numbered.len() as u32)
    }

    /// Register a class made with [`ClassTable::next_index`] under its
    /// lowercased name.
    pub(crate) fn insert(&mut self, lowercase_name: String, class: Rc<Class>) {
        debug_assert_eq!(class.index, self.next_index());
        self.has_dimensions |= (class.kind == phera_bytecode::ClassKind::Class
            && class
                .operations
                .has(super::operations::ClassOperations::ARRAY_ACCESS))
            || class
                .hooks
                .as_ref()
                .is_some_and(|hooks| hooks.dimensions.is_some());
        self.has_property_views |= class
            .hooks
            .as_ref()
            .is_some_and(|hooks| hooks.properties.is_some());
        if class.origin == super::class::ClassOrigin::Closure {
            self.closure = Some(class.index);
        }
        self.numbered.push(class.clone());
        self.entries.insert(lowercase_name, class);
    }

    /// The class with number `index`.
    #[inline]
    pub(crate) fn class(&self, index: ClassIndex) -> &Rc<Class> {
        &self.numbered[index.0 as usize]
    }

    pub(crate) fn numbered(&self, index: u32) -> Option<Rc<Class>> {
        self.numbered.get(index as usize).cloned()
    }

    /// The built-in Closure class is fixed for a request.
    pub(crate) fn closure_class(&self) -> Option<Rc<Class>> {
        let index = self.closure?;
        self.numbered.get(index.0 as usize).cloned()
    }

    /// Class identifiers ignore case, including names produced by dynamic expressions.
    pub(crate) fn get(&self, name: &str) -> Option<Rc<Class>> {
        let name = name.strip_prefix('\\').unwrap_or(name);
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
            if let Ok(mut native) = object.native.try_borrow_mut() {
                *native = None;
            }
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
