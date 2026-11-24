//! An overloaded dimension receives the original PHP offset, including null for append.
//!
//! ```php
//! $store['01'] = $value; // offsetSet('01', $value)
//! isset($store[$object]); // offsetExists($object)
//! ```

use std::rc::Rc;

use phera_bytecode::{FetchMode, Step};
use phera_extension_api::{
    Context, DimensionReferenceMode, NativeDimensions, NativeError, NativeResult,
};
use phera_value::{ArrayKey, Object, Value, to_bool};

use crate::{
    Vm,
    access_mode::Missing,
    classes::{
        Class,
        operations::{ClassOperations, NativeDimension},
    },
};

pub(crate) struct DimensionPath {
    pub(crate) object: Rc<Object>,
    pub(crate) offset: Value,
    pub(crate) suffix: Vec<Step>,
}

impl Vm {
    /// Look through already-evaluated registers and stored slots without invoking callbacks.
    /// A normal array path never crosses an overloaded object boundary.
    pub(crate) fn find_dimension(
        &self,
        root: Value,
        steps: impl Iterator<Item = Step>,
    ) -> Result<Option<DimensionPath>, NativeError> {
        let mut value = root;
        let mut steps = steps.peekable();
        while let Some(step) = steps.next() {
            let current = match value {
                Value::Reference(cell) => cell.borrow().clone(),
                other => other,
            };
            value = match (current, step) {
                (Value::Object(object), Step::Index(_) | Step::Append)
                    if has_dimension(&object) =>
                {
                    let offset = match step {
                        Step::Index(register) => self.get(register),
                        _ => Value::Null,
                    };
                    return Ok(Some(DimensionPath {
                        object,
                        offset,
                        suffix: steps.collect(),
                    }));
                }
                (Value::Array(array), Step::Index(register)) => {
                    // If this is the final step on a plain array, it can never reach
                    // an overloaded `ArrayAccess` object — skip key hashing and clone.
                    if steps.peek().is_none() {
                        return Ok(None);
                    }
                    let Ok(key) = ArrayKey::from_value(&self.operand(register)) else {
                        return Ok(None);
                    };
                    let Some(value) = array.get(&key) else {
                        return Ok(None);
                    };
                    value.clone()
                }
                (Value::Object(object), Step::Property(_) | Step::DynamicProperty(_)) => {
                    let name = match step {
                        Step::Property(name) => self.name(name)?.to_owned(),
                        Step::DynamicProperty(register) => match self.get(register) {
                            Value::InternedStr(name) => name.to_string_lossy(),
                            Value::String(name) => name.to_string_lossy(),
                            Value::Int(number) => number.to_string(),
                            _ => return Ok(None),
                        },
                        _ => return Err(internal_error!("dimension property step")),
                    };
                    let Ok(key) = self.property_key(&object, &name) else {
                        return Ok(None);
                    };
                    if let Some(class) = self.classes.for_object(&object)
                        && let Some(property) =
                            crate::classes::property_hooks::for_key(&class, &key)
                        && let Some(hooks) = &property.hooks
                        && (hooks.hooks.get.is_some() || hooks.hooks.is_virtual)
                        && !self.uses_property_backing(&object, property, hooks)
                    {
                        return Ok(None);
                    }
                    let Some(value) = object.get(&key) else {
                        return Ok(None);
                    };
                    value
                }
                _ => return Ok(None),
            };
        }
        Ok(None)
    }

    pub(crate) async fn dimension_get(
        &mut self,
        object: Rc<Object>,
        offset: Value,
    ) -> NativeResult {
        if let Some(native) = native(&object, NativeDimension::Get) {
            (native.get)(self, &object, offset)
        } else {
            self.call_protocol_method(object, "offsetGet", vec![offset])
                .await
        }
    }

    pub(crate) fn dimension_reference(
        &mut self,
        object: &Object,
        offset: Value,
        mode: DimensionReferenceMode,
    ) -> Option<NativeResult> {
        let reference = native(object, NativeDimension::Get)?.reference?;
        Some(reference(self, object, offset, mode))
    }

    pub(crate) fn bind_dimension(
        &mut self,
        object: &Object,
        offset: Value,
        value: Value,
    ) -> Option<Result<(), NativeError>> {
        let bind = native(object, NativeDimension::Set)?.bind?;
        Some(bind(self, object, offset, value))
    }

    async fn dimension_exists(
        &mut self,
        object: Rc<Object>,
        offset: Value,
    ) -> Result<bool, NativeError> {
        if let Some(native) = native(&object, NativeDimension::Exists) {
            (native.exists)(self, &object, offset)
        } else {
            Ok(to_bool(
                &self
                    .call_protocol_method(object, "offsetExists", vec![offset])
                    .await?,
            ))
        }
    }

    pub(crate) async fn dimension_set(
        &mut self,
        object: Rc<Object>,
        offset: Value,
        value: Value,
    ) -> Result<(), NativeError> {
        if let Some(native) = native(&object, NativeDimension::Set) {
            (native.set)(self, &object, offset, value)
        } else {
            self.call_protocol_method(object, "offsetSet", vec![offset, value])
                .await
                .map(drop)
        }
    }

    pub(crate) async fn dimension_unset(
        &mut self,
        object: Rc<Object>,
        offset: Value,
    ) -> Result<(), NativeError> {
        if let Some(native) = native(&object, NativeDimension::Unset) {
            (native.unset)(self, &object, offset)
        } else {
            self.call_protocol_method(object, "offsetUnset", vec![offset])
                .await
                .map(drop)
        }
    }

    pub(crate) async fn fetch_dimension(
        &mut self,
        path: DimensionPath,
        mode: FetchMode,
    ) -> NativeResult {
        let missing = if matches!(mode, FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty) {
            Missing::Silent
        } else {
            Missing::Warn
        };
        let mut steps = path.suffix.into_iter().peekable();
        let mut value = self
            .read_dimension_mode(path.object, path.offset, mode, steps.peek().is_none())
            .await?;
        while let Some(step) = steps.next() {
            let current = value.deref().clone();
            value = if let (Value::Object(object), Step::Index(register)) = (&current, step)
                && has_dimension(object)
            {
                self.read_dimension_mode(
                    object.clone(),
                    self.get(register),
                    mode,
                    steps.peek().is_none(),
                )
                .await?
            } else {
                self.ordinary_step(current, step, missing)?
            };
        }
        Ok(value.deref().clone())
    }

    pub(crate) async fn read_dimension_mode(
        &mut self,
        object: Rc<Object>,
        offset: Value,
        mode: FetchMode,
        final_step: bool,
    ) -> NativeResult {
        if matches!(mode, FetchMode::Isset | FetchMode::Quiet | FetchMode::Empty) {
            if !self
                .dimension_exists(object.clone(), offset.clone())
                .await?
            {
                return Ok(Value::Null);
            }
            if mode == FetchMode::Isset && final_step {
                return Ok(Value::Bool(true));
            }
        }
        self.dimension_get(object, offset).await
    }

    pub(crate) fn ordinary_step(
        &mut self,
        value: Value,
        step: Step,
        missing: Missing,
    ) -> NativeResult {
        match step {
            Step::Index(register) => {
                if matches!(
                    value,
                    Value::Array(_) | Value::InternedStr(_) | Value::String(_)
                ) {
                    let key = self.read_index_key(register, missing)?;
                    self.read_path_index(value, &key, missing)
                } else {
                    self.read_nonarray_offset(value, missing)
                }
            }
            Step::Append => Err(NativeError::error("Cannot use [] for reading")),
            Step::Property(name) => {
                let key = ArrayKey::str(self.name(name)?);
                self.read_property(value, key, missing)
            }
            Step::DynamicProperty(register) => {
                let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
                let name = phera_value::to_php_string(
                    &diagnostics.vm.operand(register),
                    &mut diagnostics.buffer,
                )?;
                drop(diagnostics);
                self.read_property(value, ArrayKey::str(name), missing)
            }
        }
    }

    pub(crate) fn indirect_dimension_notice(&mut self, object: &Object) {
        self.diagnostic(
            "Notice",
            format!(
                "Indirect modification of overloaded element of {} has no effect",
                object.class.name()
            ),
        );
    }
}

pub(crate) fn native(object: &Object, method: NativeDimension) -> Option<&NativeDimensions> {
    let class = object.class.as_any().downcast_ref::<Class>()?;
    if !class.operations.has(method.bit()) {
        return None;
    }
    class.hooks.as_ref()?.dimensions.as_ref()
}

pub(crate) fn has_dimension(object: &Object) -> bool {
    object
        .class
        .as_any()
        .downcast_ref::<Class>()
        .is_some_and(|class| {
            class.operations.has(ClassOperations::ARRAY_ACCESS)
                || class
                    .hooks
                    .as_ref()
                    .is_some_and(|hooks| hooks.dimensions.is_some())
        })
}
