//! Objects in a fork: the same id and class, copied contents.
//!
//! ```php
//! class Node { public ?Node $next = null; function __destruct() {} }
//! $a = new Node; $a->next = new Node; $a->next->next = $a;   // a cycle
//! Phera\snapshot();
//! // in each fork: two new Node objects with ids #1 and #2, still a cycle,
//! // each destroyed at the end of that fork's request
//! ```
//!
//! A copy is made empty, registered under its original's address so that
//! every later path to the original finds it, and queued; filling it maps
//! its properties, its native state and the values `WeakMap`s keep on it.
//! A copy whose original waits for its destructor waits for it too: the
//! fork's request destroys its own copies. A weak handle follows its
//! object into the fork if the object is copied, and is dead otherwise.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::Fork;
use crate::destruction::Destruction;
use crate::object::{Object, PoolHold};
use crate::weak_object::WeakObject;

impl Fork<'_> {
    /// The copy of an object, made empty and filled later.
    pub fn object(&mut self, original: &Rc<Object>) -> Rc<Object> {
        let address = Rc::as_ptr(original) as usize;
        if let Some(copy) = self.objects.get(&address) {
            return copy.clone();
        }
        let copy = Rc::new(Object {
            id: original.id,
            class: original.class.clone(),
            properties: RefCell::new(crate::PhpArray::new()),
            native: RefCell::new(None),
            destruction: Cell::new(Destruction::Never),
            attachments: Cell::new(None),
            pool: Some(PoolHold(self.ids.clone())),
            incarnated: false,
        });
        match original.destruction() {
            Destruction::Due => copy.expect_destructor(),
            Destruction::Done => copy.mark_destructed(),
            Destruction::Never => {}
        }
        self.objects.insert(address, copy.clone());
        self.identities.insert(original.identity(), copy.identity());
        self.unfilled_objects.push((original.clone(), copy.clone()));
        copy
    }

    /// The copy of the object at `address` in the original, if it was copied.
    pub fn copy_at(&self, address: usize) -> Option<Rc<Object>> {
        self.objects.get(&address).cloned()
    }

    /// A weak handle on the copy of the handle's object, or a dead handle
    /// when the object is gone.
    pub fn weak(&mut self, handle: &WeakObject) -> WeakObject {
        match handle.upgrade() {
            Some(object) => WeakObject::new(&self.object(&object)),
            None => WeakObject::default(),
        }
    }

    /// The identity of the copy of the object whose identity was
    /// `original`, if that object has been copied.
    pub fn identity(&self, original: usize) -> Option<usize> {
        self.identities.get(&original).copied()
    }

    pub(super) fn fill_object(&mut self, original: &Rc<Object>, copy: &Rc<Object>) {
        let properties = original.with_properties(crate::PhpArray::clone);
        let mut properties = properties.unwrap_or_default();
        self.array_contents(&mut properties);
        *copy.properties.borrow_mut() = properties;
        let native = match original.native.try_borrow() {
            Ok(native) => native.as_deref().map(|state| {
                crate::lazy_object::fork_storage(original, state, self)
                    .unwrap_or_else(|| self.native(original, state))
            }),
            Err(_) => {
                self.refuse(original, "its native state is in use");
                None
            }
        };
        if let Some(Some(native)) = native {
            *copy.native.borrow_mut() = Some(native);
        }
        if original.has_attachments() {
            self.unfilled_attachments
                .push((original.clone(), copy.clone()));
        }
    }

    /// The values `WeakMap`s keep on `original`, under the copies of those
    /// maps. A value whose map was not copied belongs to a map no fork can
    /// reach, and is left out.
    pub(super) fn fill_attachments(&mut self, original: &Rc<Object>, copy: &Rc<Object>) {
        let mut attached = Vec::new();
        original.with_attachments(|values| {
            attached.extend(values.iter().map(|(owner, value)| (*owner, value.clone())));
        });
        for (owner, value) in attached {
            if let Some(owner) = self.identity(owner) {
                let value = self.value(&value);
                copy.attach(owner, value);
            }
        }
    }

    /// After a refusal: the copies are dropped without their destructors.
    pub(super) fn discard(&mut self) {
        for copy in self.objects.values() {
            copy.mark_destructed();
        }
        self.unfilled_objects.clear();
        self.unfilled_attachments.clear();
    }

    /// Every copy. A copy that only a weak handle reached is freed now (its
    /// original was not reachable from the snapshot's roots), without its
    /// destructor, which belongs to the request that made it.
    pub(super) fn into_copies(self) -> Vec<Rc<Object>> {
        let copies: Vec<Rc<Object>> = self.objects.into_values().collect();
        for copy in &copies {
            if Rc::strong_count(copy) == 1 {
                copy.mark_destructed();
            }
        }
        copies
            .into_iter()
            .filter(|copy| Rc::strong_count(copy) > 1)
            .collect()
    }
}
