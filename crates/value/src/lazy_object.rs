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
//! Lazy-capable classes use their existing native-state field, which keeps
//! the ordinary object layout and allocation path intact.

use std::any::Any;
use std::cell::{BorrowError, BorrowMutError, Ref, RefMut};
use std::collections::HashSet;
use std::rc::Rc;

use crate::{ArrayKey, Object, PhpArray, Value};

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
        prepared: Box<dyn Any>,
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

struct LazyStorage {
    lazy: Option<Box<LazyObject>>,
    payload: Option<Box<dyn Any>>,
}

impl LazyStorage {
    fn take_values(&mut self, work: &mut Vec<Value>) {
        if let Some(lazy) = self.lazy.take() {
            lazy.into_values(work);
        }
    }
}

impl Drop for LazyStorage {
    fn drop(&mut self) {
        if self.lazy.is_some() {
            let mut work = Vec::new();
            self.take_values(&mut work);
            crate::PhpArray::drop_values(work);
        }
    }
}

/// Move the lazy wrapper's PHP values to the iterative property drop work list.
pub(crate) fn release_native(data: Option<Box<dyn Any>>, work: &mut Vec<Value>) {
    if let Some(data) = data
        && let Ok(mut lazy) = data.downcast::<LazyStorage>()
    {
        lazy.take_values(work);
    }
}

impl Object {
    /// Whether PHP regards this wrapper as an uninitialized lazy object.
    pub fn is_uninitialized_lazy(&self) -> bool {
        matches!(self.lazy().as_deref(), Some(LazyObject::Pending { .. }))
    }

    pub fn lazy(&self) -> Option<Ref<'_, LazyObject>> {
        Ref::filter_map(self.native.try_borrow().ok()?, |native| {
            native
                .as_deref()?
                .downcast_ref::<LazyStorage>()?
                .lazy
                .as_deref()
        })
        .ok()
    }

    /// Replace the lazy state while preserving bookkeeping for property references.
    pub fn replace_lazy(&self, state: Option<Box<LazyObject>>) -> Option<Box<LazyObject>> {
        let mut native = self.native.try_borrow_mut().ok()?;
        let (previous, payload) = match native.take() {
            Some(native) => match native.downcast::<LazyStorage>() {
                Ok(mut storage) => (storage.lazy.take(), storage.payload.take()),
                Err(payload) => (None, Some(payload)),
            },
            None => (None, None),
        };
        *native = match state {
            Some(state) => Some(Box::new(LazyStorage {
                lazy: Some(state),
                payload,
            })),
            None => payload,
        };
        previous
    }

    pub fn native_payload(&self) -> Result<Ref<'_, Option<Box<dyn Any>>>, BorrowError> {
        Ok(Ref::map(self.native.try_borrow()?, |native| {
            match native
                .as_ref()
                .and_then(|data| data.downcast_ref::<LazyStorage>())
            {
                Some(storage) => &storage.payload,
                None => native,
            }
        }))
    }

    pub fn native_payload_mut(&self) -> Result<RefMut<'_, Option<Box<dyn Any>>>, BorrowMutError> {
        let native = self.native.try_borrow_mut()?;
        Ok(RefMut::filter_map(native, |native| {
            Some(&mut native.as_mut()?.downcast_mut::<LazyStorage>()?.payload)
        })
        .unwrap_or_else(|native| native))
    }

    /// Raw property projections such as `(array) $proxy` follow an existing
    /// instance, without running a pending initializer.
    pub fn lazy_properties(&self) -> PhpArray {
        let Some(mut object) = self.lazy_instance() else {
            return self.to_array();
        };
        while let Some(instance) = object.lazy_instance() {
            object = instance;
        }
        object.to_array()
    }

    pub fn lazy_instance(&self) -> Option<Rc<Object>> {
        match self.lazy().as_deref() {
            Some(LazyObject::Proxy(instance)) => Some(instance.clone()),
            _ => None,
        }
    }
}
