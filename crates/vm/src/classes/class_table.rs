//! Request class lookup, leased object ids, and weak cycle-cleanup records.

use std::rc::{Rc, Weak};

use phera_value::{Object, ObjectIds};

use super::Class;

/// A stable class address shared by call contexts and unit class links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClassIndex(pub(crate) u32);

/// One request owns the id pool; weak records exist solely for cycle cleanup.
#[derive(Default)]
pub(crate) struct ClassTable {
    /// Preserve registration/declaration order for get_declared_classes.
    pub(crate) order: Vec<String>,
    pub(crate) entries: Vec<Rc<Class>>,
    pub(crate) names: crate::names::Names,
    links: Vec<Option<ClassIndex>>,
    pub(crate) objects: Vec<Weak<Object>>,
    ids: Rc<ObjectIds>,
}

impl ClassTable {
    /// Class identifiers ignore case, including names produced by dynamic expressions.
    pub(crate) fn get(&self, name: &str) -> Option<Rc<Class>> {
        self.names
            .find(name)
            .and_then(|name| self.named(name))
            .map(|index| self.at(index).clone())
    }

    /// Loaded identifiers use a flat link array instead of string lookup.
    pub(crate) fn named(&self, name: crate::names::Name) -> Option<ClassIndex> {
        self.links.get(name.0 as usize).copied().flatten()
    }

    /// An index borrows its request-owned class without changing its reference count.
    pub(crate) fn at(&self, index: ClassIndex) -> &Rc<Class> {
        &self.entries[index.0 as usize]
    }

    /// Class publication is permanent for the request, so links never invalidate.
    pub(crate) fn insert(&mut self, name: String, class: Rc<Class>) {
        let name = self.names.intern(&name);
        self.links
            .resize(self.links.len().max(name.0 as usize + 1), None);
        self.links[name.0 as usize] = Some(class.index);
        self.entries.push(class);
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

impl ClassTable {
    /// Objects of VM classes already carry their numeric request-local class identity.
    pub(crate) fn index_for_object(&self, object: &Object) -> Option<ClassIndex> {
        object
            .class
            .as_any()
            .downcast_ref::<Class>()
            .map(|class| class.index)
            .or_else(|| {
                self.names
                    .find(object.class.name())
                    .and_then(|name| self.named(name))
            })
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
        let mut table = ClassTable::native(
            &Registry::new(),
            &mut crate::signatures::Signatures::default(),
        );
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
        let table = ClassTable::native(
            &Registry::new(),
            &mut crate::signatures::Signatures::default(),
        );
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
