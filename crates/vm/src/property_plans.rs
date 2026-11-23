//! A property access that leaves the loop, done at once when there is
//! nothing to plan.
//!
//! ```php
//! $row->$column = $value;     // a name only known when it runs
//! unset($this->cache);
//! isset($entity->id);         // an uninitialized typed property: false
//! ```
//!
//! The VM's general property path is asynchronous and plans for every case
//! (magic methods, hooks, lazy objects, readonly, references) before it
//! touches the property. Most accesses have none of those. This module
//! looks at the class's [`PropertyFacts`] first, and when none of the
//! flags that matter to the access is set, it reads, writes or removes the
//! property right here: a slot of a declared property, or a key of the
//! undeclared ones.
//!
//! It takes a path of one property step (`$o->p`, `$o->$name`, `$this->p`)
//! on an object that is not lazy. It answers `false`, with nothing changed,
//! for anything that needs more, and the general path runs as before:
//! - a warning or an error to raise (an undefined property read, an
//!   uninitialized typed property read, a visibility error, a TypeError);
//! - a deprecation (creating a dynamic property where PHP warns);
//! - a PHP reference in the property (writes and unsets go through it);
//! - a coercion that runs PHP code (`__toString`);
//! - a name a static property of the class has (PHP's notice);
//! - a write or unset of a property with asymmetric visibility
//!   (`private(set)`), which checks the scope.

use phera_bytecode::{FetchMode, PathBase, Reg, Step};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Object, Value};
use std::rc::Rc;

use crate::access_mode::Missing;
use crate::classes::Class;
use crate::classes::class::PropertyDefault;
use crate::classes::property_facts::PropertyFacts;
use crate::{AccessPath, Vm};

/// The object and the property a one-step path names.
struct PlainProperty {
    object: Rc<Object>,
    class: Rc<Class>,
    key: ArrayKey,
    slot_hint: Option<usize>,
}

impl PlainProperty {
    /// The class's declaration of the property, if it declares it.
    fn declared(&self) -> Option<&PropertyDefault> {
        self.class
            .defaults
            .iter()
            .find(|declared| declared.key == self.key)
    }
}

impl Vm {
    /// `Fetch` of a one-step property path, when nothing is planned for it:
    /// the value, or for `??`, `isset` and `empty` null when the property
    /// has none. `false` when the general path must run.
    pub(crate) fn fetch_plain_property(
        &mut self,
        place: &AccessPath,
        destination: Reg,
        mode: FetchMode,
    ) -> Result<bool, NativeError> {
        let quiet = match mode {
            FetchMode::Read => false,
            FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty => true,
            FetchMode::Destructure => return Ok(false),
        };
        let mut avoid = PropertyFacts::MAGIC_GET.with(PropertyFacts::HOOKS);
        if quiet {
            avoid = avoid.with(PropertyFacts::MAGIC_ISSET);
        }
        let Some(property) = self.plain_property(place, avoid)? else {
            return Ok(false);
        };
        let value = property
            .object
            .with_properties(|properties| {
                properties
                    .get_at(&property.key, property.slot_hint)
                    .map(|v| v.deref().clone())
            })
            .flatten();
        let value = match value {
            Some(value) => value,
            None if quiet => Value::Null,
            // An undefined or uninitialized property warns or throws there.
            None => return Ok(false),
        };
        self.set(destination, value);
        Ok(true)
    }

    /// `AssignPath` of a one-step property path, when nothing is planned
    /// for it and the value fits the declared type as it is (or as PHP
    /// converts a scalar without running code). Gives the value stored,
    /// which is the assignment's result (`$x = $o->f = 1` with a `float`
    /// property is `1.0`), or `None` for the general path.
    pub(crate) fn assign_plain_property(
        &mut self,
        place: &AccessPath,
        value: &Value,
    ) -> Result<Option<Value>, NativeError> {
        let avoid = PropertyFacts::MAGIC_SET
            .with(PropertyFacts::HOOKS)
            .with(PropertyFacts::READONLY);
        if value.is_reference() {
            return Ok(None);
        }
        let Some(property) = self.plain_property(place, avoid)? else {
            return Ok(None);
        };
        let declared = property.declared();
        let value = match declared {
            Some(declared) => match self.plain_declared_value(declared, value)? {
                Some(value) => value,
                None => return Ok(None),
            },
            None => value.clone(),
        };
        let creates =
            declared.is_some() || property.class.property_facts().has(PropertyFacts::DYNAMIC);
        let written = property
            .object
            .update(
                |properties| match properties.get_mut_at(&property.key, property.slot_hint) {
                    Some(Value::Reference(_)) => false,
                    Some(slot) => {
                        *slot = value.clone();
                        true
                    }
                    None if creates => {
                        properties.insert_at(
                            property.key.clone(),
                            property.slot_hint,
                            value.clone(),
                        );
                        true
                    }
                    None => false,
                },
            )
            .unwrap_or(false);
        Ok(written.then_some(value))
    }

    /// `UnsetPath` of a one-step property path, when nothing is planned
    /// for it. A declared property's slot stays, empty.
    ///
    /// A class with any property magic keeps the general path: an unset
    /// declared property is then one its `__get`, `__set` and `__isset`
    /// answer for, which the general path records.
    pub(crate) fn unset_plain_property(&mut self, place: &AccessPath) -> Result<bool, NativeError> {
        let avoid = PropertyFacts::MAGIC_UNSET
            .with(PropertyFacts::MAGIC_GET)
            .with(PropertyFacts::MAGIC_SET)
            .with(PropertyFacts::MAGIC_ISSET)
            .with(PropertyFacts::HOOKS)
            .with(PropertyFacts::READONLY);
        let Some(property) = self.plain_property(place, avoid)? else {
            return Ok(false);
        };
        // `private(set)` and `protected(set)`: the scope check and its error.
        if property
            .declared()
            .is_some_and(|declared| declared.set_visibility.is_some())
        {
            return Ok(false);
        }
        let removed = property
            .object
            .update(
                |properties| match properties.get_at(&property.key, property.slot_hint) {
                    Some(Value::Reference(_)) => None,
                    _ => Some(properties.remove(&property.key)),
                },
            )
            .flatten();
        // The old value is dropped here, after the borrow ends.
        Ok(removed.is_some())
    }

    /// The value a write stores in a declared property: as it is when it
    /// fits the type, converted when PHP converts it without running code,
    /// `None` for the general path (an object for a string type, a type
    /// that names classes the value does not have, asymmetric visibility).
    fn plain_declared_value(
        &mut self,
        declared: &PropertyDefault,
        value: &Value,
    ) -> Result<Option<Value>, NativeError> {
        if declared.set_visibility.is_some() {
            return Ok(None);
        }
        let Some(ty) = &declared.ty else {
            return Ok(Some(value.clone()));
        };
        if crate::types::accepts_as_is(value, ty) {
            return Ok(Some(value.clone()));
        }
        if matches!(value, Value::Object(_)) {
            // A class match, or a `__toString` conversion, is the general
            // path's.
            return Ok(None);
        }
        // A scalar: PHP's conversion, or the TypeError, without PHP code.
        self.coerce_property_in_mode(value.clone(), declared, ty, None)
            .map(Some)
    }

    /// The object and key of a one-step property path whose class plans
    /// none of `avoid` for it, or `None`.
    fn plain_property(
        &mut self,
        place: &AccessPath,
        avoid: PropertyFacts,
    ) -> Result<Option<PlainProperty>, NativeError> {
        let mut steps = place.steps()?;
        let (Some(step), None) = (steps.next(), steps.next()) else {
            return Ok(None);
        };
        let object = match place.base()? {
            PathBase::Register(register) => match self.read_root(register, Missing::Silent) {
                Value::Object(object) => object,
                _ => return Ok(None),
            },
            PathBase::This { .. } => match self.current().and_then(|frame| frame.this().cloned()) {
                Some(object) => object,
                None => return Ok(None),
            },
            _ => return Ok(None),
        };
        if object.lazy().is_some() {
            return Ok(None);
        }
        let Some(class) = object
            .class
            .as_any()
            .downcast_ref::<Class>()
            .and_then(|class| class.identity.upgrade())
        else {
            return Ok(None);
        };
        if class.property_facts().has_any(avoid) {
            return Ok(None);
        }
        // The name is borrowed, from the file's names or from the string
        // the program built, not copied.
        let unit;
        let dynamic;
        let name: &str = match step {
            Step::Property(name) => {
                let frame = self
                    .current()
                    .ok_or_else(|| internal_error!("property path"))?;
                unit = frame.unit().clone();
                match unit.names.get(name.index()) {
                    Some(name) => name,
                    None => return Ok(None),
                }
            }
            Step::DynamicProperty(register) => match self.read_root(register, Missing::Silent) {
                Value::InternedStr(name) => match std::str::from_utf8(name.as_bytes()) {
                    Ok(name) => name,
                    Err(_) => return Ok(None),
                },
                Value::String(name) => {
                    dynamic = name;
                    match std::str::from_utf8(dynamic.as_bytes()) {
                        Ok(name) => name,
                        Err(_) => return Ok(None),
                    }
                }
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        if name.is_empty()
            || name.starts_with('\0')
            || class
                .static_properties
                .iter()
                .any(|property| property.name == name)
        {
            return Ok(None);
        }
        let Ok((key, slot_hint)) = self.resolve_property_key(&object, name) else {
            return Ok(None);
        };
        Ok(Some(PlainProperty {
            object,
            class,
            key,
            slot_hint,
        }))
    }
}
