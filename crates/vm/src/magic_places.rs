//! `$object->items['key']` walks properties and offsets in source order.
//! The last step of isset/empty uses a different hook protocol from ??.

use phera_bytecode::{FetchMode, PathBase, Step};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Object, Value};
use std::rc::Rc;

use crate::access_mode::{Creation, Missing};
use crate::inline_stack::InlineStack;
use crate::magic_properties::{PropertyRead, PropertyReadSource};
use crate::path_update::PathKey;
use crate::{AccessPath, Vm};

pub(crate) struct ReadPlace {
    pub(crate) value: Value,
    pub(crate) target: Option<UpdateTarget>,
}

pub(crate) enum UpdateTarget {
    Indirect {
        root: Value,
        keys: Box<InlineStack<PathKey>>,
    },
    Property {
        object: Rc<Object>,
        key: ArrayKey,
    },
}

struct IndirectPath {
    root: Value,
    keys: InlineStack<PathKey>,
}

#[derive(PartialEq, Eq)]
enum ReadPurpose {
    Value,
    Update,
}

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
        Ok(self
            .read_magic_path(place, mode, ReadPurpose::Value)
            .await?
            .value)
    }

    pub(crate) async fn read_for_path_update(
        &mut self,
        place: &AccessPath,
    ) -> Result<ReadPlace, NativeError> {
        self.read_magic_path(place, FetchMode::Read, ReadPurpose::Update)
            .await
    }

    async fn read_magic_path(
        &mut self,
        place: &AccessPath,
        mode: FetchMode,
        purpose: ReadPurpose,
    ) -> Result<ReadPlace, NativeError> {
        let mut indirect: Option<IndirectPath> = None;
        let mut target = None;
        let quiet = matches!(mode, FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty);
        let missing = if quiet {
            Missing::Silent
        } else {
            Missing::Warn
        };
        let Some(register) = self.base_register(place, Creation::Existing, missing)? else {
            return Ok(ReadPlace {
                value: Value::Null,
                target,
            });
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
                return Ok(ReadPlace {
                    value: Value::Null,
                    target,
                });
            }
            let (key, converted) = self.magic_path_key(step).await?;
            let last = steps.peek().is_none();
            if purpose == ReadPurpose::Update && converted && !last {
                indirect = Some(IndirectPath {
                    root: value.clone(),
                    keys: InlineStack::new(),
                });
            }
            if let Some(indirect) = &mut indirect {
                let resolved = match (&key, &value) {
                    (PathKey::Property(key), Value::Object(object)) => {
                        let name = crate::path_update::property_name(key);
                        PathKey::Property(
                            self.property_key(object, &name)
                                .unwrap_or_else(|_| key.clone()),
                        )
                    }
                    _ => key.clone(),
                };
                indirect.keys.push(resolved);
            }
            value = match key {
                PathKey::Property(key) => {
                    let read = match (mode, steps.peek()) {
                        (FetchMode::Isset, None) => PropertyRead::Isset,
                        (FetchMode::Empty, None) => PropertyRead::Empty,
                        _ if quiet => PropertyRead::Coalesce,
                        _ => PropertyRead::Read,
                    };
                    let owner = match &value {
                        Value::Object(object) if purpose == ReadPurpose::Update && !last => {
                            Some(object.clone())
                        }
                        _ => None,
                    };
                    if purpose == ReadPurpose::Update
                        && last
                        && (converted || indirect.is_some())
                        && let Value::Object(object) = &value
                    {
                        target = Some(UpdateTarget::Property {
                            object: object.clone(),
                            key: key.clone(),
                        });
                        indirect = None;
                    }
                    let name = owner
                        .as_ref()
                        .map(|_| crate::path_update::property_name(&key));
                    let result = self.resolve_property_read(value, key, read).await?;
                    if result.source == PropertyReadSource::Getter
                        && let (Some(owner), Some(name)) = (owner, name)
                    {
                        self.warn_indirect_magic_property(&owner, &name, &result.value);
                        indirect = Some(IndirectPath {
                            root: result.value.clone(),
                            keys: InlineStack::new(),
                        });
                    }
                    match result.value {
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
        let target = target.or_else(|| {
            indirect.map(|path| UpdateTarget::Indirect {
                root: path.root,
                keys: Box::new(path.keys),
            })
        });
        Ok(ReadPlace { value, target })
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
