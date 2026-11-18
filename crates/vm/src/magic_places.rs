//! `$object->items['key']` walks properties and offsets in source order.
//! The last step of isset/empty uses a different hook protocol from ??.

use phera_bytecode::{FetchMode, PathBase, Step};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Value};

use crate::access_mode::{Creation, Missing};
use crate::magic_properties::PropertyRead;
use crate::path_update::PathKey;
use crate::{AccessPath, Vm};

impl AccessPath {
    pub(crate) fn has_properties(&self) -> Result<bool, NativeError> {
        Ok(self
            .steps()?
            .any(|step| matches!(step, Step::Property(_) | Step::DynamicProperty(_))))
    }
}

impl Vm {
    pub(crate) async fn read_magic_place(
        &mut self,
        place: &AccessPath,
        mode: FetchMode,
    ) -> NativeResult {
        let quiet = matches!(mode, FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty);
        let missing = if quiet {
            Missing::Silent
        } else {
            Missing::Warn
        };
        let Some(register) = self.base_register(place, Creation::Existing, missing)? else {
            return Ok(Value::Null);
        };
        if matches!(place.base()?, PathBase::StaticProperty { .. }) {
            self.check_static_read(place, missing)?;
        }
        let mut value = self.read_root(register, missing);
        let mut steps = place.steps()?.peekable();
        while let Some(step) = steps.next() {
            if quiet
                && !value.is_object()
                && matches!(step, Step::Property(_) | Step::DynamicProperty(_))
            {
                return Ok(Value::Null);
            }
            let (key, _) = self.magic_path_key(step).await?;
            value = match key {
                PathKey::Property(key) => {
                    let read = match (mode, steps.peek()) {
                        (FetchMode::Isset, None) => PropertyRead::Isset,
                        (FetchMode::Empty, None) => PropertyRead::Empty,
                        _ if quiet => PropertyRead::Coalesce,
                        _ => PropertyRead::Read,
                    };
                    let result = self.read_overloaded_property(value, key, read).await?;
                    match result {
                        Value::Reference(reference) => {
                            let value = reference.borrow().clone();
                            self.release_property_reference(reference);
                            value
                        }
                        value => value,
                    }
                }
                PathKey::Index(key) => {
                    let key = key.ok_or_else(|| NativeError::error("Cannot use [] for reading"))?;
                    self.read_path_index(value, &key, missing)?
                }
            };
        }
        Ok(value)
    }

    /// Resolve dynamic property names as strings. A dimension key keeps
    /// its own PHP conversion rules, which never use __toString.
    pub(crate) async fn magic_path_key(
        &mut self,
        step: Step,
    ) -> Result<(PathKey, bool), NativeError> {
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let key = match step {
            Step::Property(name) => PathKey::Property(ArrayKey::str(diagnostics.vm.name(name)?)),
            Step::Index(index) => PathKey::Index(Some(ArrayKey::from_value_diag(
                &diagnostics.vm.operand(index),
                &mut diagnostics.buffer,
            )?)),
            Step::Append => PathKey::Index(None),
            Step::DynamicProperty(register) => {
                let value = diagnostics.vm.get(register);
                if value.is_object() {
                    drop(diagnostics);
                    let text = self.stringify_value(&value).await?;
                    return Ok((PathKey::Property(ArrayKey::String(text)), true));
                }
                PathKey::Property(ArrayKey::String(phera_value::to_php_string(
                    &value,
                    &mut diagnostics.buffer,
                )?))
            }
        };
        Ok((key, false))
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
            other => {
                if missing == Missing::Warn {
                    self.warn(format!(
                        "Trying to access array offset on {}",
                        other.type_name()
                    ));
                }
                Value::Null
            }
        };
        Ok(value)
    }
}
