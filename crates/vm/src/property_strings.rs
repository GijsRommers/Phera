//! A weak typed property assignment performs the same Stringable conversion
//! as a weak typed argument, for example `$object->label = $stringable`.

use phera_extension_api::NativeResult;
use phera_value::{ArrayKey, Object, Value};

use crate::Vm;
use crate::classes::Class;

impl Vm {
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
