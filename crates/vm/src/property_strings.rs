//! A weak typed property assignment performs the same Stringable conversion
//! as a weak typed argument, for example `$object->label = $stringable`.

use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Object, Value};

use crate::Vm;
use crate::classes::Class;

impl Vm {
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
                property.declaring, property.name, ty.display
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
