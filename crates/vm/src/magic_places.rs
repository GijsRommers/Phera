//! `$object->items['key']` walks properties and offsets in source order.
//! The last step of isset/empty uses a different hook protocol from ??.

use phera_bytecode::{FetchMode, Reg, Step};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Object, Value};
use std::rc::Rc;

use crate::access_mode::{Creation, Missing, Mutation};
use crate::classes::property_hooks::HookRead;
use crate::code_table::FunctionIndex;
use crate::inline_stack::InlineStack;
use crate::magic_properties::{PropertyRead, PropertyReadSource, PropertyReadStep};
use crate::path_update::PathKey;
use crate::{AccessPath, Vm};

pub(crate) struct ReadPlace {
    pub(crate) value: Value,
    pub(crate) target: Option<UpdateTarget>,
}

pub(crate) enum UpdateTarget {
    Resolved {
        register: Reg,
        keys: Box<InlineStack<PathKey>>,
    },
    Indirect {
        root: Value,
        keys: Box<InlineStack<PathKey>>,
    },
    Property {
        object: Rc<Object>,
        key: ArrayKey,
    },
    Dimension(Rc<Object>, Value),
}

struct IndirectPath {
    root: Value,
    keys: InlineStack<PathKey>,
}

pub(crate) enum PathReadStep {
    Complete(ReadPlace),
    Lazy(Rc<Object>),
    Getter {
        function: FunctionIndex,
        object: Rc<Object>,
        indirect: Option<String>,
    },
}

pub(crate) struct PathRead {
    pub(crate) place: AccessPath,
    mode: FetchMode,
    register: Option<Reg>,
    next: usize,
    retry: Option<PathKey>,
    value: Value,
    indirect: Option<Box<IndirectPath>>,
    resolved: Option<Box<InlineStack<PathKey>>>,
    target: Option<UpdateTarget>,
}

impl AccessPath {
    pub(crate) fn has_properties(&self) -> Result<bool, NativeError> {
        Ok(self
            .steps()?
            .any(|step| matches!(step, Step::Property(_) | Step::DynamicProperty(_))))
    }
}

impl Vm {
    pub(crate) fn start_read_path(
        &mut self,
        place: AccessPath,
        mode: FetchMode,
        update: bool,
    ) -> Result<PathRead, NativeError> {
        let missing = fetch_missing(mode);
        let register = self.base_register(&place, Creation::Existing, missing)?;
        if register.is_some() && crate::static_places::static_base(place.base()?).is_some() {
            self.check_static_read(&place, missing)?;
        }
        let value = register.map_or(Value::Null, |register| self.read_root(register, missing));
        Ok(PathRead {
            place,
            mode,
            register,
            next: 0,
            retry: None,
            value,
            indirect: None,
            resolved: update.then(|| Box::new(InlineStack::new())),
            target: None,
        })
    }

    pub(crate) async fn advance_path_read(
        &mut self,
        path: &mut PathRead,
    ) -> Result<PathReadStep, NativeError> {
        let Some(register) = path.register else {
            return Ok(PathReadStep::Complete(ReadPlace {
                value: Value::Null,
                target: None,
            }));
        };
        let mode = path.mode;
        let quiet = matches!(mode, FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty);
        let missing = fetch_missing(mode);
        let update = path.resolved.is_some();
        let mut value = std::mem::take(&mut path.value);
        let mut steps = path.place.steps()?.skip(path.next).peekable();
        while let Some(step) = steps.next() {
            path.next += 1;
            if let Step::Index(index) = step
                && let Value::Object(object) = &value
                && crate::dimensions::has_dimension(object)
            {
                let offset = self.get(index);
                let object = object.clone();
                let last = steps.peek().is_none();
                if update && last {
                    path.target = Some(UpdateTarget::Dimension(object.clone(), offset.clone()));
                }
                value = self.read_dimension_mode(object, offset, mode, last).await?;
                continue;
            }
            if quiet
                && !value.is_object()
                && matches!(step, Step::Property(_) | Step::DynamicProperty(_))
            {
                return Ok(PathReadStep::Complete(ReadPlace {
                    value: Value::Null,
                    target: path.target.take(),
                }));
            }
            let key = if let Some(key) = path.retry.take() {
                key
            } else if let Step::Index(index) = step {
                if update {
                    PathKey::Index(Some(self.write_index_key(&value, index)?))
                } else if matches!(value, Value::Array(_) | Value::String(_)) {
                    PathKey::Index(Some(self.read_index_key(index, missing)?))
                } else {
                    // No offset is converted if the container cannot be indexed.
                    value = self.read_nonarray_offset(value, missing)?;
                    continue;
                }
            } else {
                self.magic_path_key(step).await?
            };
            if let (PathKey::Property(name), Value::Object(object)) = (&key, &value) {
                use crate::lazy_property_access::LazyAccess;
                let access = match (mode, steps.peek()) {
                    (FetchMode::Quiet, _) | (FetchMode::Isset | FetchMode::Empty, Some(_)) => {
                        LazyAccess::Coalesce
                    }
                    (FetchMode::Isset | FetchMode::Empty, None) => LazyAccess::Isset,
                    _ => LazyAccess::Read,
                };
                let effective = self.lazy_property_receiver(object, name, access);
                if self.property_requires_initialization(&effective, name, access) {
                    path.value = value;
                    path.retry = Some(key);
                    path.next -= 1;
                    return Ok(PathReadStep::Lazy(effective));
                }
                if !Rc::ptr_eq(object, &effective) {
                    value = Value::Object(effective);
                    if update {
                        path.indirect = Some(Box::new(IndirectPath {
                            root: value.clone(),
                            keys: InlineStack::new(),
                        }));
                    }
                }
            }
            let last = steps.peek().is_none();
            if update {
                let stored = match (&key, &value) {
                    (PathKey::Property(key), Value::Object(object)) => {
                        let name = crate::path_update::property_name(key);
                        PathKey::Property(
                            self.property_key(object, &name)
                                .unwrap_or_else(|_| key.clone()),
                        )
                    }
                    _ => key.clone(),
                };
                match &mut path.indirect {
                    Some(path) => path.keys.push(stored),
                    None => {
                        if let Some(keys) = &mut path.resolved {
                            keys.push(stored);
                        }
                    }
                }
            }
            value = match key {
                PathKey::Property(key) => {
                    let read = match (mode, steps.peek()) {
                        _ if update => PropertyRead::Update,
                        (FetchMode::Isset, None) => PropertyRead::Isset,
                        (FetchMode::Empty, None) => PropertyRead::Empty,
                        _ if quiet => PropertyRead::Coalesce,
                        _ => PropertyRead::Read,
                    };
                    let owner = match &value {
                        Value::Object(object) if update && !last => Some(object.clone()),
                        _ => None,
                    };
                    if update
                        && last
                        && let Value::Object(object) = &value
                    {
                        path.target = Some(UpdateTarget::Property {
                            object: object.clone(),
                            key: key.clone(),
                        });
                        path.indirect = None;
                    }
                    let name = owner
                        .as_ref()
                        .map(|_| crate::path_update::property_name(&key));
                    if let Some(object) = &owner
                        && let Some(class) = self.classes.for_object(object)
                        && let Some(slot) = self
                            .property_slot(object, &crate::path_update::property_name(&key))
                            .1
                        && let Some(property) =
                            crate::classes::property_hooks::for_key(&class, &slot)
                        && let Some(hooks) = &property.hooks
                    {
                        match self.declared_hook_read(object, property, hooks, true)? {
                            HookRead::Getter(function) => {
                                return Ok(PathReadStep::Getter {
                                    function,
                                    object: object.clone(),
                                    indirect: Some(property.name.clone()),
                                });
                            }
                            HookRead::Value(hooked) => {
                                value = hooked.deref().clone();
                                path.indirect = Some(Box::new(IndirectPath {
                                    root: hooked,
                                    keys: InlineStack::new(),
                                }));
                                continue;
                            }
                            HookRead::Storage => {}
                        }
                    }
                    if let Some(object) = &owner
                        && matches!(steps.peek(), Some(Step::Index(_) | Step::Append))
                        && let Some(class) = self.classes.for_object(object)
                        && class.property_access.is_none()
                        && let (stored, Some(slot)) =
                            self.property_slot(object, &crate::path_update::property_name(&key))
                        && (stored.is_some()
                            || self.typed_property_uninitialized(&class, object, &slot)
                            || self
                                .property_hook(
                                    &class,
                                    object,
                                    &key,
                                    crate::magic_properties::PropertyHook::Get,
                                )
                                .is_none())
                    {
                        value = self
                            .prepare_property_dimension(object, &slot, Mutation::Assign)?
                            .deref()
                            .clone();
                        continue;
                    }
                    let result = match self.plan_property_read(value, key, read).await? {
                        PropertyReadStep::Value(result) => result,
                        PropertyReadStep::Hook(function, object) => {
                            return Ok(PathReadStep::Getter {
                                function,
                                object,
                                indirect: None,
                            });
                        }
                    };
                    if result.source == PropertyReadSource::Getter
                        && let (Some(owner), Some(name)) = (owner, name)
                    {
                        self.warn_indirect_magic_property(&owner, &name, &result.value);
                        path.indirect = Some(Box::new(IndirectPath {
                            root: result.value.clone(),
                            keys: InlineStack::new(),
                        }));
                    }
                    self.read_path_value(result.value)
                }
                PathKey::Index(key) => {
                    let key = key.ok_or_else(|| NativeError::error("Cannot use [] for reading"))?;
                    if update {
                        self.read_writable_index(value, &key)?
                    } else {
                        self.read_path_index(value, &key, missing)?
                    }
                }
            };
        }
        let target = path.target.take().or_else(|| {
            path.resolved.take().map(|keys| match path.indirect.take() {
                Some(path) => UpdateTarget::Indirect {
                    root: path.root,
                    keys: Box::new(path.keys),
                },
                None => UpdateTarget::Resolved { register, keys },
            })
        });
        Ok(PathReadStep::Complete(ReadPlace { value, target }))
    }

    pub(crate) fn resume_path_read(
        &mut self,
        path: &mut PathRead,
        value: Value,
        indirect: Option<(Rc<Object>, String)>,
    ) -> Result<(), NativeError> {
        path.value = if let Some((object, name)) = indirect {
            if !value.is_reference() && !value.is_object() {
                return Err(crate::classes::property_hooks::indirect_error(
                    &object, &name,
                ));
            }
            let read = value.deref().clone();
            path.indirect = Some(Box::new(IndirectPath {
                root: value,
                keys: InlineStack::new(),
            }));
            read
        } else {
            self.read_path_value(value)
        };
        Ok(())
    }

    fn read_path_value(&mut self, value: Value) -> Value {
        match value {
            Value::Reference(reference) => {
                let value = reference.borrow().clone();
                self.release_property_reference(reference);
                value
            }
            value => value,
        }
    }

    /// Resolve dynamic property names as strings. A dimension key keeps
    /// its own PHP conversion rules, which never use __toString.
    pub(crate) async fn magic_path_key(&mut self, step: Step) -> Result<PathKey, NativeError> {
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
                    return Ok(PathKey::Property(ArrayKey::String(text)));
                }
                PathKey::Property(ArrayKey::String(phera_value::to_php_string(
                    &value,
                    &mut diagnostics.buffer,
                )?))
            }
        };
        Ok(key)
    }
}

fn fetch_missing(mode: FetchMode) -> Missing {
    match mode {
        FetchMode::Isset | FetchMode::Empty => Missing::Isset,
        FetchMode::Quiet => Missing::Silent,
        _ => Missing::Warn,
    }
}
