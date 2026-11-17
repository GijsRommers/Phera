//! Resolve guarded property receivers without constructing PHP property-name strings.

use crate::{
    AccessPath, Vm,
    access_mode::{Creation, Missing},
};
use phera_bytecode::{NameId, PathBase, Step};
use phera_extension_api::NativeError;
use phera_value::{Object, Value};
use std::rc::Rc;

/// A property receiver and its compiler-interned name.
pub(crate) struct PropertyTarget {
    pub(crate) object: Rc<Object>,
    pub(crate) name: NameId,
}

/// An assignment and an append have distinct validation requirements.
pub(crate) enum PropertyShape {
    Single,
    Append,
}

impl Vm {
    /// Array and dynamic paths return immediately without consulting the site cache.
    pub(crate) fn property_target(
        &mut self,
        place: &AccessPath,
        shape: PropertyShape,
    ) -> Result<Option<PropertyTarget>, NativeError> {
        let mut steps = place.steps()?;
        let Some(Step::Property(name)) = steps.next() else {
            return Ok(None);
        };
        if matches!(shape, PropertyShape::Append) && steps.next() != Some(Step::Append) {
            return Ok(None);
        }
        if steps.next().is_some() {
            return Ok(None);
        }
        if !matches!(place.base()?, PathBase::Register(_) | PathBase::This { .. }) {
            return Ok(None);
        }
        let Some(register) = self.base_register(place, Creation::Existing, Missing::Silent)? else {
            return Ok(None);
        };
        let Value::Object(object) = self.read_root(register, Missing::Silent) else {
            return Ok(None);
        };
        Ok(Some(PropertyTarget { object, name }))
    }

    /// Mirrors read_place for an existing object at one integer array index.
    /// No key conversion can warn on this path; every other shape/value falls back.
    pub(crate) fn indexed_property_target(
        &mut self,
        place: &AccessPath,
    ) -> Result<Option<PropertyTarget>, NativeError> {
        let mut steps = place.steps()?;
        let (Some(Step::Index(index)), Some(Step::Property(name)), None) =
            (steps.next(), steps.next(), steps.next())
        else {
            return Ok(None);
        };
        let PathBase::Register(root) = place.base()? else {
            return Ok(None);
        };
        let Value::Int(index) = self.get(index) else {
            return Ok(None);
        };
        let Value::Array(array) = self.get(root) else {
            return Ok(None);
        };
        let Some(value) = crate::array_slots::read_integer(&array, index) else {
            return Ok(None);
        };
        let Value::Object(object) = &*value.deref() else {
            return Ok(None);
        };
        Ok(Some(PropertyTarget {
            object: object.clone(),
            name,
        }))
    }
}
