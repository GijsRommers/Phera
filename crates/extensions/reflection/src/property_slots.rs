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
//! - the object keeps the property in that slot (its class is the
//!   property's class or extends it);
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

/// The reflected member of a `ReflectionProperty` object, and its slot.
fn member(reflection: &Object) -> Option<Rc<MemberInfo>> {
    let native = reflection.native.try_borrow().ok()?;
    let member = native.as_ref()?.downcast_ref::<Rc<MemberInfo>>()?.clone();
    member.slot_access.is_some().then_some(member)
}

/// The object a call is about: its first argument, when it is one that is
/// not lazy.
fn target(arguments: &[Option<Value>]) -> Option<&Rc<Object>> {
    match arguments.first()? {
        Some(Value::Object(object)) if object.lazy().is_none() => Some(object),
        _ => None,
    }
}

/// What the object holds in the property's slot: `None` outside when it
/// does not keep the property there.
fn read<R>(
    access: &SlotAccess,
    object: &Object,
    answer: impl FnOnce(Option<&Value>) -> Option<R>,
) -> Option<R> {
    object
        .with_properties(|properties| access.read(properties).and_then(answer))
        .flatten()
}

/// Stores `value` in the property's slot, for a plain write (see the module).
fn write(access: &SlotAccess, object: &Object, value: &Value) -> Option<Value> {
    if !access.accepts(value) {
        return None;
    }
    let magic = object.class.has_property_magic();
    object
        .update(|properties| {
            match access.read(properties)? {
                Some(Value::Reference(_)) => return None,
                None if magic => return None,
                _ => {}
            }
            properties.set_slot(access.slot, value.clone())?;
            Some(Value::Null)
        })
        .flatten()
}

/// `isInitialized($object)`.
pub(crate) fn is_initialized(reflection: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    let [_] = arguments else {
        return None;
    };
    let member = member(reflection)?;
    let access = member.slot_access.as_ref()?;
    let object = target(arguments)?;
    // An unset property of a class with `__isset` asks it.
    let magic = object.class.has_property_magic();
    read(access, object, |value| {
        (value.is_some() || !magic).then(|| Value::Bool(value.is_some()))
    })
}

/// `getValue($object)` and `getRawValue($object)`: a property with a value.
pub(crate) fn get_value(reflection: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    let [_] = arguments else {
        return None;
    };
    let member = member(reflection)?;
    let access = member.slot_access.as_ref()?;
    read(access, target(arguments)?, |value| {
        value.map(|value| value.deref().clone())
    })
}

/// `isLazy($object)`: `false` for an object that is not lazy.
pub(crate) fn is_lazy(reflection: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    let [_] = arguments else {
        return None;
    };
    let member = member(reflection)?;
    let access = member.slot_access.as_ref()?;
    read(access, target(arguments)?, |_| Some(Value::Bool(false)))
}

/// `setValue($object, $value)`, `setRawValue($object, $value)` and
/// `setRawValueWithoutLazyInitialization($object, $value)`, which are the
/// same write for a property without hooks of an object that is not lazy.
pub(crate) fn set_value(reflection: &Object, arguments: &[Option<Value>]) -> Option<Value> {
    let [_, Some(value)] = arguments else {
        return None;
    };
    let member = member(reflection)?;
    let access = member.slot_access.as_ref()?;
    write(access, target(arguments)?, value)
}
