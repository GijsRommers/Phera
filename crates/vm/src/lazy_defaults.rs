//! `$reflection->markLazyObjectAsInitialized($ghost)` restores the defaults
//! of lazy slots, leaving any slot populated through reflection intact.

use std::collections::HashSet;

use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Object, PhpArray, Value};

use crate::Vm;
use crate::classes::class::PropertyDefault;

impl Vm {
    // PhpString's cached hash cell does not change Eq or Hash.
    #[allow(clippy::mutable_key_type)]
    pub(crate) fn restore_lazy_defaults(
        &mut self,
        object: &Object,
        keys: &HashSet<ArrayKey>,
    ) -> Result<(), NativeError> {
        let class = self
            .classes
            .for_object(object)
            .ok_or_else(|| internal_error!("lazy defaults class"))?;
        let old = object.to_array();
        let mut merged = PhpArray::new();
        for property in &class.defaults {
            if keys.contains(&property.key) {
                if let Some(value) = self.lazy_property_default(property)? {
                    merged.insert(property.key.clone(), value);
                }
            } else if let Some(value) = old.get(&property.key) {
                merged.insert(property.key.clone(), value.clone());
            }
        }
        for (key, value) in old.iter() {
            if !merged.contains_key(&key) && !keys.contains(&key) {
                merged.insert(key, value.clone());
            }
        }
        object.update(|storage| storage.replace_all(merged));
        Ok(())
    }

    pub(crate) fn lazy_property_default(
        &mut self,
        property: &PropertyDefault,
    ) -> Result<Option<Value>, NativeError> {
        if !property.initialized {
            return Ok(None);
        }
        let value = match &property.initializer {
            Some(initializer) => self.evaluate_property_initializer(initializer, property)?,
            None => property.value.clone(),
        };
        Ok(Some(value))
    }
}
