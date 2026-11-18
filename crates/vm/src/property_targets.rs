//! Resolve guarded property receivers without constructing PHP property-name strings.

use crate::{AccessPath, Vm, access_mode::Missing};
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
        self.literal_property_target(place.base()?, name)
    }

    /// This fast path reads the receiver directly, leaving the compiler's
    /// temporary register for the general path that actually needs it.
    fn literal_property_target(
        &mut self,
        base: PathBase,
        name: NameId,
    ) -> Result<Option<PropertyTarget>, NativeError> {
        let object = match base {
            PathBase::Register(register) => match self.read_root(register, Missing::Silent) {
                Value::Object(object) => object,
                _ => return Ok(None),
            },
            PathBase::This { .. } => self
                .current()
                .and_then(|frame| frame.this().cloned())
                .ok_or_else(|| NativeError::error("Using $this when not in object context"))?,
            _ => return Ok(None),
        };
        Ok(Some(PropertyTarget { object, name }))
    }

    /// Parse either cached read shape in one pass over the access plan.
    pub(crate) fn read_property_target(
        &mut self,
        place: &AccessPath,
    ) -> Result<Option<PropertyTarget>, NativeError> {
        let mut steps = place.steps()?;
        match (steps.next(), steps.next(), steps.next()) {
            (Some(Step::Property(name)), None, _) => {
                self.literal_property_target(place.base()?, name)
            }
            (Some(Step::Index(index)), Some(Step::Property(name)), None) => {
                let PathBase::Register(root) = place.base()? else {
                    return Ok(None);
                };
                Ok(self.indexed_property_target(root, index, name))
            }
            _ => Ok(None),
        }
    }

    /// Mirrors read_place for an existing object at one integer array index.
    /// No key conversion can warn on this path; every other shape/value falls back.
    fn indexed_property_target(
        &self,
        root: phera_bytecode::Reg,
        index: phera_bytecode::Reg,
        name: NameId,
    ) -> Option<PropertyTarget> {
        let index = self.operand(index);
        let Value::Int(index) = &*index else {
            return None;
        };
        let root = self.operand(root);
        let Value::Array(array) = &*root else {
            return None;
        };
        let value = crate::array_slots::read_integer(array, *index)?;
        let Value::Object(object) = &*value.deref() else {
            return None;
        };
        Some(PropertyTarget {
            object: object.clone(),
            name,
        })
    }
}
