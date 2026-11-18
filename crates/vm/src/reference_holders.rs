//! Slot-owned leases keep the reference index exact without retaining PHP values or objects.

use std::{cell::RefCell, collections::HashMap, rc::Rc, rc::Weak};

use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Object, Value};

use crate::classes::class::PropertyDefault;

/// The weak cell also keeps its allocation identity reserved until the last slot leaves.
struct ReferenceEntry {
    reference: Weak<RefCell<Value>>,
    holders: Vec<ReferenceHolder>,
}

struct ReferenceHolder {
    identity: u64,
    object: Option<Weak<Object>>,
    key: ArrayKey,
}

/// Untyped holders support alias separation; only typed holders enter the write-check map.
#[derive(Default)]
pub(crate) struct ReferenceHolders {
    entries: HashMap<usize, ReferenceEntry>,
    pub(crate) constraints: HashMap<usize, Vec<(u64, Rc<PropertyDefault>)>>,
    next_identity: u64,
}

/// Dropping a slot, static storage, or the owning object's native payload releases its lease.
pub(crate) struct PropertyBinding {
    table: Weak<RefCell<ReferenceHolders>>,
    reference: usize,
    identity: u64,
}

/// Created lazily on the first property alias; ordinary objects allocate no bookkeeping.
#[derive(Default)]
struct ObjectReferences {
    slots: HashMap<ArrayKey, PropertyBinding>,
}

/// Inherited static declarations share this mutable slot, including reference rebinding.
pub(crate) struct StaticProperty {
    pub(crate) reference: RefCell<Rc<RefCell<Value>>>,
    binding: RefCell<Option<PropertyBinding>>,
}

impl StaticProperty {
    pub(crate) fn new(value: Value) -> Rc<Self> {
        Rc::new(Self {
            reference: RefCell::new(Rc::new(RefCell::new(value))),
            binding: RefCell::new(None),
        })
    }

    pub(crate) fn track(&self, table: &Rc<RefCell<ReferenceHolders>>, property: &PropertyDefault) {
        if self.binding.borrow().is_none() {
            let binding = ReferenceHolders::bind(
                table,
                &self.reference.borrow(),
                None,
                &property.key,
                Some(property),
            );
            *self.binding.borrow_mut() = Some(binding);
        }
    }

    pub(crate) fn rebind(
        &self,
        reference: Rc<RefCell<Value>>,
        table: &Rc<RefCell<ReferenceHolders>>,
        property: &PropertyDefault,
    ) {
        if Rc::ptr_eq(&self.reference.borrow(), &reference) {
            self.track(table, property);
            return;
        }
        self.binding.borrow_mut().take();
        *self.reference.borrow_mut() = reference;
        self.track(table, property);
    }
}

impl ReferenceHolders {
    /// Ordinary object cloning owns property cells; other native payloads
    /// require their class's own duplication function.
    pub(crate) fn has_extension_payload(object: &Object) -> bool {
        object
            .native
            .borrow()
            .as_ref()
            .is_some_and(|value| !value.is::<ObjectReferences>())
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.constraints.is_empty()
    }

    fn bind(
        table: &Rc<RefCell<Self>>,
        reference: &Rc<RefCell<Value>>,
        object: Option<&Rc<Object>>,
        key: &ArrayKey,
        property: Option<&PropertyDefault>,
    ) -> PropertyBinding {
        let address = Rc::as_ptr(reference) as usize;
        let mut index = table.borrow_mut();
        let identity = index.next_identity;
        index.next_identity += 1;
        let holder = ReferenceHolder {
            identity,
            object: object.map(Rc::downgrade),
            key: key.clone(),
        };
        index
            .entries
            .entry(address)
            .or_insert_with(|| ReferenceEntry {
                reference: Rc::downgrade(reference),
                holders: Vec::new(),
            })
            .holders
            .push(holder);
        if let Some(property) = property.filter(|property| property.ty.is_some()) {
            let mut declaration = property.clone();
            // Metadata must never own the indexed cell, static slot, or default object graph.
            declaration.value = Value::Null;
            declaration.static_storage = None;
            declaration.initializer = None;
            index
                .constraints
                .entry(address)
                .or_default()
                .push((identity, Rc::new(declaration)));
        }
        PropertyBinding {
            table: Rc::downgrade(table),
            reference: address,
            identity,
        }
    }

    /// Native-backed instances have no spare payload slot for an object-lifetime lease.
    /// Reject typed bindings explicitly rather than silently losing their constraints.
    pub(crate) fn check_object_storage(
        object: &Object,
        key: &ArrayKey,
    ) -> Result<bool, NativeError> {
        let Some(class) = object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
        else {
            return Ok(false);
        };
        let native = object.native.borrow();
        let occupied = native
            .as_ref()
            .is_some_and(|payload| !payload.is::<ObjectReferences>());
        let native_backed = class.is_a("PDO")
            || class.is_a("PDOStatement")
            || class.origin == crate::classes::class::ClassOrigin::Closure;
        if !occupied && !native_backed {
            return Ok(true);
        }
        if let Some(property) = class
            .defaults
            .iter()
            .find(|property| property.key == *key && property.ty.is_some())
        {
            return Err(NativeError::error(format!(
                "Typed property references on native-backed objects are unsupported: {}::${}",
                property.declaring, property.name
            )));
        }
        Ok(false)
    }

    /// Called only where a property slot acquires a reference, never for ordinary value writes.
    pub(crate) fn track_object(
        table: &Rc<RefCell<Self>>,
        object: &Rc<Object>,
        key: &ArrayKey,
        reference: &Rc<RefCell<Value>>,
    ) -> Result<(), NativeError> {
        if !Self::check_object_storage(object, key)? {
            return Ok(());
        }
        let Some(class) = object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
        else {
            return Ok(());
        };
        let property = class.defaults.iter().find(|property| property.key == *key);
        let mut native = object.native.borrow_mut();
        if native.is_none() {
            *native = Some(Box::<ObjectReferences>::default());
        }
        let slots = native
            .as_mut()
            .and_then(|native| native.downcast_mut::<ObjectReferences>())
            .ok_or_else(|| internal_error!("property reference conflicts with native payload"))?;
        if let Some(binding) = slots.slots.get(key)
            && binding.reference == Rc::as_ptr(reference) as usize
        {
            return Ok(());
        }
        slots.slots.remove(key);
        let binding = Self::bind(table, reference, Some(object), key, property);
        slots.slots.insert(key.clone(), binding);
        Ok(())
    }

    pub(crate) fn untrack_object(object: &Object, key: &ArrayKey) {
        if let Some(slots) = object
            .native
            .borrow_mut()
            .as_mut()
            .and_then(|native| native.downcast_mut::<ObjectReferences>())
        {
            slots.slots.remove(key);
        }
    }

    pub(crate) fn sole_object_holder(
        &self,
        reference: &Rc<RefCell<Value>>,
    ) -> Option<(Rc<Object>, ArrayKey)> {
        let entry = self.entries.get(&(Rc::as_ptr(reference) as usize))?;
        if entry.holders.len() != 1 || !entry.reference.ptr_eq(&Rc::downgrade(reference)) {
            return None;
        }
        let holder = entry.holders.first()?;
        Some((holder.object.as_ref()?.upgrade()?, holder.key.clone()))
    }
}

impl Drop for PropertyBinding {
    fn drop(&mut self) {
        let Some(table) = self.table.upgrade() else {
            return;
        };
        let mut index = table.borrow_mut();
        if let Some(constraints) = index.constraints.get_mut(&self.reference) {
            constraints.retain(|(identity, _)| *identity != self.identity);
            if constraints.is_empty() {
                index.constraints.remove(&self.reference);
            }
        }
        if let Some(entry) = index.entries.get_mut(&self.reference) {
            entry
                .holders
                .retain(|holder| holder.identity != self.identity);
            if entry.holders.is_empty() {
                index.entries.remove(&self.reference);
            }
        }
    }
}
