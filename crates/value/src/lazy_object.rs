//! Property storage for reflection-created lazy objects.
//!
//! ```php
//! class Record { public int $id; public string $name; }
//! $record = (new ReflectionClass(Record::class))->newLazyGhost(
//!     static function (Record $record) { $record->name = 'Loaded'; }
//! );
//! (new ReflectionProperty(Record::class, 'id'))
//!     ->setRawValueWithoutLazyInitialization($record, 42);
//! echo $record->id;   // Reads the populated slot without initialization.
//! echo $record->name; // A missing slot reaches the general property handler.
//! ```
//!
//! Cached reads already fall back for absent slots. Cached writes already call
//! a separate policy for absent slots. Lazy objects omit lazy slots, so hits on
//! populated ordinary properties use the same code. A resolved proxy keeps its
//! own table empty and resolves the effective receiver on the general path.
//! Hook caches already dispatch separately from reads and writes of storage.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::{ArrayKey, Object, Value};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LazyStrategy {
    Ghost,
    Proxy,
}

/// Allocated only for objects created or reset through Reflection.
pub enum LazyObject {
    Pending {
        strategy: LazyStrategy,
        initializer: Value,
        properties: HashSet<ArrayKey>,
        skip_serialization: bool,
    },
    Initializing,
    Proxy(Rc<Object>),
}

impl LazyObject {
    pub(crate) fn into_values(self, work: &mut Vec<Value>) {
        match self {
            Self::Pending { initializer, .. } => work.push(initializer),
            Self::Proxy(instance) => work.push(Value::Object(instance)),
            Self::Initializing => {}
        }
    }
}

#[derive(Default)]
pub(crate) struct LazyStorage(RefCell<Option<Box<LazyObject>>>);

impl LazyStorage {
    pub(crate) fn take_values(&mut self, work: &mut Vec<Value>) {
        if let Some(lazy) = self.0.get_mut().take() {
            lazy.into_values(work);
        }
    }
}

impl Drop for LazyStorage {
    fn drop(&mut self) {
        if self.0.get_mut().is_some() {
            let mut work = Vec::new();
            self.take_values(&mut work);
            crate::PhpArray::drop_values(work);
        }
    }
}

impl Object {
    /// Whether PHP regards this wrapper as an uninitialized lazy object.
    pub fn is_uninitialized_lazy(&self) -> bool {
        matches!(self.lazy().as_deref(), Some(LazyObject::Pending { .. }))
    }

    pub fn lazy(&self) -> Option<std::cell::Ref<'_, LazyObject>> {
        std::cell::Ref::filter_map(self.lazy.0.try_borrow().ok()?, |state| state.as_deref()).ok()
    }

    /// Replace the state before entering or after returning from PHP code.
    pub fn replace_lazy(&self, state: Option<Box<LazyObject>>) -> Option<Box<LazyObject>> {
        self.lazy
            .0
            .try_borrow_mut()
            .ok()
            .and_then(|mut current| std::mem::replace(&mut *current, state))
    }

    pub fn lazy_instance(&self) -> Option<Rc<Object>> {
        match self.lazy().as_deref() {
            Some(LazyObject::Proxy(instance)) => Some(instance.clone()),
            _ => None,
        }
    }
}
