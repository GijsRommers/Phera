//! `ReflectionProperty`'s reads and writes of an object's property, run by
//! the interpreter where the call is (direct methods, no call frame).
//!
//! ```php
//! $rp = new ReflectionProperty(Product::class, 'name');
//! $rp->setRawValueWithoutLazyInitialization($product, 'Desk');  // ORM hydration
//! $rp->isInitialized($product);
//! ```
//!
//! The reflected member carries where an object keeps the property
//! ([`SlotAccess`]: its key and slot in the class's layout). A handler here
//! reads or writes that slot when the call is the plain case:
//! - the object keeps the property in that slot and is an instance of the
//!   property's class (the layout of the class the reflector was made for
//!   shows it at once; any other object is asked);
//! - the object is not lazy;
//! - a write stores a value of a type the declaration accepts as it is,
//!   into a slot that holds no PHP reference, and a property that is unset
//!   only when the class has no property magic (`__set`).
//!
//! Every other case answers `None`, and the method runs as before, with
//! PHP's errors, conversions, hooks and lazy initialization.

use std::rc::Rc;

use phera_extension_api::reflection::{MemberInfo, SlotAccess};
use phera_extension_api::{Object, Value};

/// A reflected property and where an object keeps it.
struct Slot<'a> {
    member: &'a MemberInfo,
    access: &'a SlotAccess,
}

/// Runs `run` with the property a `ReflectionProperty` object reflects, if
/// it is one an object keeps in a slot. The member stays borrowed from the
/// reflector, not copied out of it: a call spends a few instructions here.
fn with_slot<R>(reflection: &Object, run: impl FnOnce(Slot<'_>) -> Option<R>) -> Option<R> {
    let native = reflection.native.try_borrow().ok()?;
    let member = &**native.as_ref()?.downcast_ref::<Rc<MemberInfo>>()?;
    run(Slot {
        member,
        access: member.slot_access.as_ref()?,
    })
}

/// The object a call is about: its first argument, when it is one that is
/// not lazy.
fn target(arguments: &[Option<Value>]) -> Option<&Rc<Object>> {
    match arguments.first()? {
        Some(Value::Object(object)) if object.lazy().is_none() => Some(object),
        _ => None,
    }
}

impl Slot<'_> {
    /// What the object holds in the property's slot: `None` outside when it
    /// is not an object of the property's class, or does not keep the
    /// property there.
    fn read<R>(
        &self,
        object: &Object,
        answer: impl FnOnce(Option<&Value>) -> Option<R>,
    ) -> Option<R> {
        object
            .with_properties(|properties| {
                let held = self.access.read(properties)?;
                if !self.is_of_class(object, properties) {
                    return None;
                }
                answer(held)
            })
            .flatten()
    }

    /// Whether the object is of the class that has the property, as PHP
    /// requires of the object a reflector reads or writes. An object of the
    /// layout the reflector was made for is; any other is asked.
    fn is_of_class(
        &self,
        object: &Object,
        properties: &phera_extension_api::value::Properties,
    ) -> bool {
        self.access.is_own_layout(properties) || object.class.is_instance_of(&self.member.class)
    }

    /// Stores `value` in the property's slot, for a plain write (see the module).
    fn write(&self, object: &Object, value: &Value) -> Option<Value> {
        if !self.access.accepts(value) {
            return None;
        }
        object
            .update(|properties| {
                match self.access.read(properties)? {
                    Some(Value::Reference(_)) => return None,
                    // An unset property of a class with `__set` asks it.
                    None if object.class.has_property_magic() => return None,
                    _ => {}
                }
                if !self.is_of_class(object, properties) {
                    return None;
                }
                properties.set_slot(self.access.slot, value.clone())?;
                Some(Value::Null)
            })
            .flatten()
    }
}

/// `isInitialized($object)`.
pub(crate) fn is_initialized(reflection: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    let [_] = arguments else {
        return None;
    };
    let object = target(arguments)?;
    with_slot(reflection, |slot| {
        slot.read(object, |value| {
            // An unset property of a class with `__isset` asks it.
            (value.is_some() || !object.class.has_property_magic())
                .then(|| Value::Bool(value.is_some()))
        })
    })
}

/// `getValue($object)` and `getRawValue($object)`: a property with a value.
pub(crate) fn get_value(reflection: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    let [_] = arguments else {
        return None;
    };
    let object = target(arguments)?;
    with_slot(reflection, |slot| {
        slot.read(object, |value| value.map(|value| value.deref().clone()))
    })
}

/// `isLazy($object)`: `false` for an object that is not lazy.
pub(crate) fn is_lazy(reflection: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    let [_] = arguments else {
        return None;
    };
    let object = target(arguments)?;
    with_slot(reflection, |slot| {
        slot.read(object, |_| Some(Value::Bool(false)))
    })
}

/// `setValue($object, $value)`, `setRawValue($object, $value)` and
/// `setRawValueWithoutLazyInitialization($object, $value)`, which are the
/// same write for a property without hooks of an object that is not lazy.
pub(crate) fn set_value(reflection: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    let [_, Some(value)] = arguments else {
        return None;
    };
    let object = target(arguments)?;
    with_slot(reflection, |slot| slot.write(object, value))
}
