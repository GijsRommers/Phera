//! Indexed property paths validate the container before converting an offset.
//! `$object->items[1.5]++` converts the key once for both the read and the write.

use phera_bytecode::Reg;
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Value};

use crate::{
    Vm,
    access_mode::{Missing, Mutation},
    places::display_key,
};

impl Vm {
    pub(crate) fn path_index_key(&mut self, register: Reg) -> Result<ArrayKey, NativeError> {
        self.warn_undefined_key(register);
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        Ok(ArrayKey::from_value_diag(
            &diagnostics.vm.operand(register),
            &mut diagnostics.buffer,
        )?)
    }

    pub(crate) fn read_index_key(
        &mut self,
        register: Reg,
        missing: Missing,
    ) -> Result<ArrayKey, NativeError> {
        self.path_index_key(register).map_err(|error| {
            if missing == Missing::Isset {
                let kind = self.operand(register).type_name();
                NativeError::type_error(format!(
                    "Cannot access offset of type {kind} in isset or empty"
                ))
            } else {
                error
            }
        })
    }

    /// PHP validates the container before converting a write's offset.
    pub(crate) fn write_index_key(
        &mut self,
        current: &Value,
        index: Reg,
    ) -> Result<ArrayKey, NativeError> {
        match current {
            Value::Array(_) | Value::Null | Value::String(_) => {}
            Value::Bool(false) => self.diagnostic(
                "Deprecated",
                "Automatic conversion of false to array is deprecated".into(),
            ),
            Value::Object(object) => {
                return Err(NativeError::error(format!(
                    "Cannot use object of type {} as array",
                    object.class.name()
                )));
            }
            _ => return Err(NativeError::error("Cannot use a scalar value as an array")),
        }
        self.path_index_key(index)
    }

    /// During read-modify-write, a missing parent dimension becomes an array.
    pub(crate) fn read_writable_index(&mut self, current: Value, key: &ArrayKey) -> NativeResult {
        if matches!(current, Value::Null | Value::Bool(false)) {
            self.warn(format!("Undefined array key {}", display_key(key)));
            return Ok(Value::Null);
        }
        self.read_path_index(current, key, Missing::Warn)
    }

    /// Unset accepts a null key without the diagnostic used by reads and writes.
    pub(crate) fn unset_index_key(
        &mut self,
        current: &Value,
        register: Reg,
    ) -> Result<Option<ArrayKey>, NativeError> {
        self.warn_undefined_key(register);
        match current {
            Value::Array(_) => {}
            Value::Null => return Ok(None),
            Value::Bool(false) => {
                self.diagnostic(
                    "Deprecated",
                    "Automatic conversion of false to array is deprecated".into(),
                );
                return Ok(None);
            }
            Value::String(_) => return Err(NativeError::error("Cannot unset string offsets")),
            Value::Object(object) => {
                return Err(NativeError::error(format!(
                    "Cannot use object of type {} as array",
                    object.class.name()
                )));
            }
            _ => {
                return Err(NativeError::error(
                    "Cannot unset offset in a non-array variable",
                ));
            }
        }
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let key = diagnostics.vm.operand(register);
        if key.is_null() {
            Ok(Some(ArrayKey::str("")))
        } else {
            ArrayKey::from_value_diag(&key, &mut diagnostics.buffer)
                .map(Some)
                .map_err(|_| {
                    NativeError::type_error(format!(
                        "Cannot unset offset of type {} on array",
                        key.type_name()
                    ))
                })
        }
    }

    /// A writable dimension first acquires its property slot, then converts its key.
    pub(crate) fn prepare_property_dimension(
        &mut self,
        object: &phera_value::Object,
        key: &phera_value::ArrayKey,
        mutation: Mutation,
    ) -> Result<Value, NativeError> {
        let class = self
            .classes
            .for_object(object)
            .ok_or_else(|| internal_error!("property class"))?;
        let stored = crate::property_storage::read(object, key);
        let property = class.defaults.iter().find(|property| property.key == *key);
        if let Some(property) = property.filter(|property| property.is_readonly)
            && !stored.as_ref().is_some_and(Value::is_object)
        {
            self.check_readonly_property_write(object, property, true, mutation)?;
        }
        let name = crate::path_update::property_name(key);
        if let Some(property) = property
            && mutation != Mutation::Unset
            && stored
                .as_ref()
                .is_none_or(|value| matches!(&*value.deref(), Value::Null | Value::Bool(false)))
        {
            check_property_array_promotion(property)?;
        }
        if let Some(value) = stored {
            return Ok(value);
        }
        if property.is_some() {
            if mutation == Mutation::Unset {
                return Ok(Value::Null);
            }
        } else {
            if class.is_readonly || class.origin == crate::classes::class::ClassOrigin::Closure {
                return Err(NativeError::error(format!(
                    "Cannot create dynamic property {}::${name}",
                    class.name
                )));
            }
            if !class.allows_dynamic_properties {
                self.diagnostic(
                    "Deprecated",
                    format!(
                        "Creation of dynamic property {}::${name} is deprecated",
                        class.name
                    ),
                );
            }
        }
        let value = if mutation == Mutation::Unset {
            Value::Null
        } else {
            Value::from(phera_value::PhpArray::new())
        };
        crate::property_storage::write(object, key.clone(), value.clone());
        Ok(value)
    }

    pub(crate) fn read_path_index(
        &mut self,
        value: Value,
        key: &ArrayKey,
        missing: Missing,
    ) -> NativeResult {
        let value = match value {
            Value::Array(array) => {
                if let Some(value) = array.get(key) {
                    value.deref().clone()
                } else {
                    if missing == Missing::Warn {
                        self.warn(format!(
                            "Undefined array key {}",
                            crate::places::display_key(key)
                        ));
                    }
                    Value::Null
                }
            }
            Value::String(string) => self.read_string_offset(&string, key.clone(), missing)?,
            other => return self.read_nonarray_offset(other, missing),
        };
        Ok(value)
    }

    /// Scalar or absent containers have no offset to convert.
    pub(crate) fn read_nonarray_offset(&mut self, value: Value, missing: Missing) -> NativeResult {
        if let Value::Object(object) = value {
            return Err(NativeError::error(format!(
                "Cannot use object of type {} as array",
                object.class.name()
            )));
        }
        if missing == Missing::Warn {
            self.warn(format!(
                "Trying to access array offset on {}",
                value.type_name()
            ));
        }
        Ok(Value::Null)
    }
}

pub(crate) fn accepts_promoted_array(ty: &phera_bytecode::Type) -> bool {
    ty.mask
        .intersects(phera_bytecode::TypeMask::ARRAY.union(phera_bytecode::TypeMask::ITERABLE))
}

pub(crate) fn check_property_array_promotion(
    property: &crate::classes::class::PropertyDefault,
) -> Result<(), NativeError> {
    if let Some(ty) = &property.ty
        && !accepts_promoted_array(ty)
    {
        return Err(NativeError::type_error(format!(
            "Cannot auto-initialize an array inside property {}::${} of type {}",
            phera_value::class_display_name(&property.declaring),
            property.name,
            crate::types::php_type_display(ty)
        )));
    }
    Ok(())
}
