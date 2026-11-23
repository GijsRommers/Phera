//! A weak typed property assignment performs the same Stringable conversion
//! as a weak typed argument, for example `$object->label = $stringable`.

use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Object, Value};

use crate::classes::Class;
use crate::{AccessPath, Vm};

impl Vm {
    /// A native setter runs synchronously. Perform any Stringable call in the
    /// VM before handing the converted value to the setter.
    pub(crate) async fn native_string_assignment(
        &mut self,
        access: &phera_extension_api::NativePropertyAccess,
        object: &Object,
        name: &str,
        value: &Value,
    ) -> Result<Option<Value>, NativeError> {
        let Some(property) = access
            .string_property
            .and_then(|lookup| lookup(object, name))
        else {
            return Ok(None);
        };
        let strict = self
            .current()
            .is_some_and(|frame| frame.function().strict_types);
        match &*value.deref() {
            Value::InternedStr(_) | Value::String(_) => return Ok(None),
            Value::Null if property.nullable => return Ok(None),
            Value::Int(_) | Value::Float(_) | Value::Bool(_) if !strict => return Ok(None),
            Value::Object(_) if !strict && Self::has_string_conversion(value) => {}
            _ => {
                let expected = if property.nullable {
                    "?string"
                } else {
                    "string"
                };
                return Err(NativeError::type_error(format!(
                    "Cannot assign {} to property {}::${name} of type {expected}",
                    value.type_name(),
                    property.declaring
                )));
            }
        }
        self.coerce_stringable(value.clone()).await.map(Some)
    }

    /// Static properties verify visibility before a weak string cast runs PHP.
    pub(crate) async fn cast_static_property_value(
        &mut self,
        place: &AccessPath,
        value: &Value,
    ) -> Result<Option<Value>, NativeError> {
        if !Self::has_string_conversion(value)
            || self
                .current()
                .is_some_and(|frame| frame.function().strict_types)
            || place.steps()?.next().is_some()
        {
            return Ok(None);
        }
        let Some((class, name)) = crate::static_places::static_base(place.base()?) else {
            return Ok(None);
        };
        let (class_name, property_name) = self.static_property_names(class, name)?;
        let scope = self
            .classes
            .get(&class_name)
            .ok_or_else(|| Self::class_miss(&class_name, "Class"))?;
        let ty = scope
            .static_properties
            .iter()
            .rev()
            .find(|property| property.name == property_name)
            .and_then(|property| property.ty.as_ref());
        if ty.is_some_and(|ty| {
            self.needs_string_coercion(value, ty, crate::types::TypeChecking::Coerce)
        }) {
            self.static_property(class, name)?;
            self.coerce_stringable(value.clone()).await.map(Some)
        } else {
            Ok(None)
        }
    }

    /// An explicitly unset typed property still checks a magic getter's result.
    pub(crate) async fn validate_getter_value(
        &mut self,
        class: &Class,
        slot: Option<&ArrayKey>,
        mut result: Value,
    ) -> NativeResult {
        let Some((property, ty)) = slot
            .and_then(|key| class.defaults.iter().find(|property| property.key == *key))
            .and_then(|property| property.ty.as_ref().map(|ty| (property, ty)))
        else {
            return Ok(result);
        };
        let mode = self
            .current()
            .is_some_and(|frame| frame.function().strict_types)
            .into();
        if self.needs_string_coercion(&result, ty, mode) {
            result = self.coerce_stringable(result).await?;
        }
        let converted = self.coerce_property(result.deref().clone(), property, ty).map_err(|_| {
            let actual = result.type_name();
            let declaring = class.magic.as_ref().and_then(|magic| magic.get.as_ref())
                .map_or(class.name.as_str(), |method| method.declaring.as_str());
            NativeError::type_error(format!(
                "Value of type {actual} returned from {declaring}::__get() must be compatible with unset property {}::${} of type {}",
                property.declaring,
                property.name,
                crate::types::php_type_display(ty)
            ))
        })?;
        if let Value::Reference(reference) = &result {
            *reference
                .try_borrow_mut()
                .map_err(|_| internal_error!("magic property reference"))? = converted;
            Ok(result)
        } else {
            Ok(converted)
        }
    }

    pub(crate) fn property_needs_string(
        &self,
        object: &Object,
        key: &ArrayKey,
        value: &Value,
    ) -> bool {
        if self
            .current()
            .is_some_and(|frame| frame.function().strict_types)
            || !Self::has_string_conversion(value)
        {
            return false;
        }
        object
            .class
            .as_any()
            .downcast_ref::<Class>()
            .is_some_and(|class| {
                class
                    .defaults
                    .iter()
                    .find(|property| property.key == *key)
                    .and_then(|property| property.ty.as_ref())
                    .is_some_and(|ty| {
                        self.needs_string_coercion(value, ty, crate::types::TypeChecking::Coerce)
                    })
            })
    }

    pub(crate) async fn property_string(
        &mut self,
        object: &Object,
        key: &ArrayKey,
        value: Value,
    ) -> NativeResult {
        if self.property_needs_string(object, key, &value) {
            if let Some(property) = object
                .class
                .as_any()
                .downcast_ref::<Class>()
                .and_then(|class| class.defaults.iter().find(|property| property.key == *key))
                .filter(|property| property.is_readonly)
            {
                self.check_readonly_property_write(
                    object,
                    property,
                    false,
                    crate::access_mode::Mutation::Assign,
                )?;
            }
            self.coerce_stringable(value).await
        } else {
            Ok(value)
        }
    }
}
