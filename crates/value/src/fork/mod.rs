//! Copying a request's values for a fork of a snapshot.
//!
//! ```php
//! $config = new Config(['debug' => false]);
//! $a = $config;                 // two variables, one object
//! $list = [&$counter, $config]; // a reference cell and the same object again
//! Phera\snapshot();             // every request forked from here gets its own
//!                               // Config, still shared by $a, $config and $list[1],
//!                               // and its own $counter cell
//! ```
//!
//! A fork must see exactly the graph the snapshot holds, and nothing it does
//! may reach the snapshot or another fork. So every object and every
//! reference cell reachable from the snapshot is copied once: a second
//! path to the same object leads to the same copy, which keeps sharing and
//! cycles as they were. A copy keeps its object's id (`spl_object_id`,
//! `var_dump`'s `#3`) and its class, which is immutable and shared.
//!
//! Strings are shared. An array is shared too (it is copy-on-write) unless
//! it holds an object or a reference somewhere inside, see [`arrays`].
//!
//! Copying never recurses over the data: an object or a cell is made empty
//! first and queued, and filled later, so a chain of a million objects is a
//! loop of a million steps, not a million stack frames. State that native
//! code keeps on an object (an `ArrayObject`'s storage, a closure's bound
//! variables) is copied by the class's own fork hook ([`NativeFork`]),
//! which may refuse: an open file cannot be in two requests.

mod arrays;
mod objects;

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::object::{Object, ObjectIds, Resource};
use crate::value::Value;

/// Copies the native state of objects of built-in classes; implemented by
/// the VM, which knows each class's fork hook.
pub trait NativeFork {
    /// A copy of `state`, the native state of `object`, with every value
    /// inside replaced through `fork`; or why this object cannot be copied.
    fn fork_native(
        &self,
        object: &Object,
        state: &dyn Any,
        fork: &mut Fork<'_>,
    ) -> Result<Box<dyn Any>, String>;

    /// A copy of an open resource (a stream), or why it cannot be copied.
    fn fork_resource(&self, resource: &Resource, fork: &mut Fork<'_>) -> Result<Resource, String>;
}

/// Why a value could not be copied, and which one it was.
#[derive(Debug, Clone)]
pub struct Refusal {
    /// The class of the object (or the kind of the resource) refused.
    pub class: String,
    /// What makes it impossible, for the exception message.
    pub reason: String,
    /// The address of the refused object or resource in the original
    /// graph, so that the caller can find the path to it from a root.
    pub address: usize,
}

/// A PHP reference cell.
type Cell = Rc<RefCell<Value>>;

struct NativeCopy {
    _source: Rc<dyn Any>,
    copy: Rc<dyn Any>,
}

/// One copy in progress. Make it with [`Fork::new`], map each root with
/// [`Fork::value`], then call [`Fork::finish`].
pub struct Fork<'n> {
    pub(crate) ids: Rc<ObjectIds>,
    natives: &'n dyn NativeFork,
    /// Copies by the address of the original object.
    pub(crate) objects: HashMap<usize, Rc<Object>>,
    /// The identity (see `Object::identity`) of each copy, by the identity
    /// of its original: `WeakMap` entries are tagged with identities.
    pub(crate) identities: HashMap<usize, usize>,
    references: HashMap<usize, Rc<RefCell<Value>>>,
    resources: HashMap<usize, Rc<Resource>>,
    pub(crate) arrays: HashMap<usize, Rc<crate::PhpArray>>,
    native_copies: Option<HashMap<(TypeId, usize), NativeCopy>>,
    /// Copies made but not filled yet.
    pub(crate) unfilled_objects: Vec<(Rc<Object>, Rc<Object>)>,
    unfilled_references: Vec<(Cell, Cell)>,
    /// Copies whose originals carry `WeakMap` values; filled once every
    /// map has its copy.
    pub(crate) unfilled_attachments: Vec<(Rc<Object>, Rc<Object>)>,
    refusal: Option<Refusal>,
}

impl<'n> Fork<'n> {
    /// A copy into a new id pool that starts where `ids` is: the same ids
    /// are taken, and the same ones are free for reuse.
    pub fn new(ids: &ObjectIds, natives: &'n dyn NativeFork) -> Self {
        Fork {
            ids: ids.fork(),
            natives,
            objects: HashMap::new(),
            identities: HashMap::new(),
            references: HashMap::new(),
            resources: HashMap::new(),
            arrays: HashMap::new(),
            native_copies: None,
            unfilled_objects: Vec::new(),
            unfilled_references: Vec::new(),
            unfilled_attachments: Vec::new(),
            refusal: None,
        }
    }

    /// The id pool of the copies.
    pub fn ids(&self) -> &Rc<ObjectIds> {
        &self.ids
    }

    /// Copy native backing shared by several PHP objects once for this fork.
    /// For example, two views of one XML document, and every node object of
    /// a DOM document, must keep sharing the fork's one copied tree. `copy`
    /// gets the fork, to copy the objects the backing refers to.
    pub fn shared_native<T: 'static>(
        &mut self,
        source: &Rc<T>,
        copy: impl FnOnce(&T, &mut Self) -> T,
    ) -> Rc<T> {
        let key = (TypeId::of::<T>(), Rc::as_ptr(source) as usize);
        if let Some(copied) = self
            .native_copies
            .as_ref()
            .and_then(|copies| copies.get(&key))
            && let Ok(value) = copied.copy.clone().downcast::<T>()
        {
            return value;
        }
        let value = Rc::new(copy(source, self));
        let copies = self.native_copies.get_or_insert_with(HashMap::new);
        copies.insert(
            key,
            NativeCopy {
                _source: source.clone(),
                copy: value.clone(),
            },
        );
        value
    }

    /// The copy of a value. Scalars and strings are themselves.
    pub fn value(&mut self, value: &Value) -> Value {
        match value {
            Value::Array(array) => Value::Array(self.array(array)),
            Value::Object(object) => Value::Object(self.object(object)),
            Value::Reference(cell) => Value::Reference(self.reference(cell)),
            Value::Resource(resource) => Value::Resource(self.resource(resource)),
            other => other.clone(),
        }
    }

    /// The copy of a resource: a closed one is shared (it can no longer
    /// change), an open one is copied by its extension, once.
    pub fn resource(&mut self, resource: &Rc<Resource>) -> Rc<Resource> {
        if resource.is_closed() {
            return resource.clone();
        }
        let address = Rc::as_ptr(resource) as usize;
        if let Some(copy) = self.resources.get(&address) {
            return copy.clone();
        }
        let natives = self.natives;
        let copy = match natives.fork_resource(resource, self) {
            Ok(copy) => Rc::new(copy),
            Err(reason) => {
                self.refuse_address(address, resource.kind, reason);
                resource.clone()
            }
        };
        self.resources.insert(address, copy.clone());
        copy
    }

    /// The copy of a reference cell, made empty and filled later.
    pub fn reference(&mut self, cell: &Rc<RefCell<Value>>) -> Rc<RefCell<Value>> {
        let address = Rc::as_ptr(cell) as usize;
        if let Some(copy) = self.references.get(&address) {
            return copy.clone();
        }
        let copy = Rc::new(RefCell::new(Value::Null));
        self.references.insert(address, copy.clone());
        self.unfilled_references.push((cell.clone(), copy.clone()));
        copy
    }

    /// Records that a value cannot be copied. The first refusal is the one
    /// reported; copying goes on so that the caller can drop the copy.
    pub fn refuse(&mut self, object: &Object, reason: impl Into<String>) {
        self.refuse_address(
            std::ptr::from_ref(object) as usize,
            object.class.name(),
            reason,
        );
    }

    /// Records that a part of the request's state that is not a value
    /// (an iterator in the middle of a loop, for example) cannot be copied.
    pub fn refuse_state(&mut self, what: &str, reason: impl Into<String>) {
        self.refuse_address(0, what, reason);
    }

    fn refuse_address(&mut self, address: usize, class: &str, reason: impl Into<String>) {
        self.refusal.get_or_insert_with(|| Refusal {
            class: class.to_owned(),
            reason: reason.into(),
            address,
        });
    }

    /// Fills every copy made so far, and every copy that filling makes.
    /// Returns the copies of all objects, or the first refusal.
    pub fn finish(mut self) -> Result<Vec<Rc<Object>>, Refusal> {
        self.drain();
        if let Some(refusal) = self.refusal.take() {
            self.discard();
            return Err(refusal);
        }
        Ok(self.into_copies())
    }

    /// Fills every copy made so far, and every copy that filling makes, so
    /// that [`Fork::identity`] knows every object reachable from the values
    /// mapped so far.
    pub fn drain(&mut self) {
        loop {
            if let Some((original, copy)) = self.unfilled_objects.pop() {
                self.fill_object(&original, &copy);
            } else if let Some((original, copy)) = self.unfilled_references.pop() {
                let inner = original.try_borrow().map(|value| value.clone());
                let inner = inner.unwrap_or_default();
                let inner = self.value(&inner);
                *copy.borrow_mut() = inner;
            } else if let Some((original, copy)) = self.unfilled_attachments.pop() {
                self.fill_attachments(&original, &copy);
            } else {
                break;
            }
        }
    }

    /// The native state of `object` copied through its class's hook.
    pub(crate) fn native(&mut self, object: &Object, state: &dyn Any) -> Option<Box<dyn Any>> {
        // A copy of the shared reference, so that the fork can be lent out.
        let natives = self.natives;
        match natives.fork_native(object, state, self) {
            Ok(copy) => Some(copy),
            Err(reason) => {
                self.refuse(object, reason);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests;
