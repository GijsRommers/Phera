//! `parent::$name::set($value)` either enters the parent's function or
//! addresses the receiver's backing property through PHP's raw fallback.

use std::rc::Rc;

use phera_bytecode::Visibility;
use phera_extension_api::{NativeError, NativeResult};
use phera_value::Value;

use super::{Class, property_hooks::property_error};
use crate::Vm;
use crate::calls::call_state::{CallTarget, Callee, ParameterSpec};
use crate::calls::callback_binding::ArgumentBinding;
use crate::intrinsics::Intrinsic;

#[derive(Clone, Copy)]
pub(crate) enum ParentHook {
    Get,
    Set,
}

impl ParentHook {
    pub(crate) fn parse(name: &str) -> Option<(&str, Self)> {
        let (property, kind) = name.strip_prefix('$')?.rsplit_once("::")?;
        match kind {
            "get" => Some((property, Self::Get)),
            "set" => Some((property, Self::Set)),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Set => "set",
        }
    }
}

impl Vm {
    pub(super) fn prepare_parent_property_hook(
        &mut self,
        parent: &Class,
        property: &str,
        operation: ParentHook,
    ) -> Result<(), NativeError> {
        let parent_name = phera_value::class_display_name(&parent.name);
        let selected = parent
            .defaults
            .iter()
            .rev()
            .find(|candidate| candidate.name == property)
            .ok_or_else(|| {
                NativeError::error(format!("Undefined property {parent_name}::${property}"))
            })?;
        if selected.visibility == Visibility::Private {
            return Err(NativeError::error(format!(
                "Cannot access private property {parent_name}::${property}"
            )));
        }
        let object = self
            .current()
            .and_then(|frame| frame.this().cloned())
            .ok_or_else(|| NativeError::error("Using $this when not in object context"))?;
        let hook = selected.hooks.as_ref().and_then(|hooks| match operation {
            ParentHook::Get => hooks.hooks.get.as_ref(),
            ParentHook::Set => hooks.hooks.set.as_ref(),
        });
        if let Some(hook) = hook {
            if hook.is_abstract {
                return Err(NativeError::error(format!(
                    "Cannot call abstract method {}()",
                    self.code.function(hook.function).name,
                )));
            }
            return self.prepare_declared_hook(hook.function, object, None);
        }
        let parameters = match operation {
            ParentHook::Get => Vec::new(),
            ParentHook::Set => vec![ParameterSpec {
                name: "value".into(),
                ty: None,
                by_reference: false,
                requires_reference: false,
                variadic: false,
            }],
        };
        self.prepare_call(Callee {
            name: format!("{parent_name}::${property}::{}", operation.name()).into(),
            parameters: parameters.into(),
            argument_binding: ArgumentBinding::Caller,
            target: CallTarget::Intrinsic(Intrinsic::ParentProperty(operation)),
        })
    }

    pub(crate) async fn parent_property_fallback(
        &mut self,
        operation: ParentHook,
        arguments: Vec<Value>,
    ) -> NativeResult {
        let (object, parent, name) = self
            .current()
            .and_then(|frame| {
                let parent = frame.scope()?.parent.clone()?;
                let (_, method) = frame.function().name.split_once("::")?;
                let (property, _) = ParentHook::parse(method)?;
                Some((frame.this()?.clone(), parent, property.to_owned()))
            })
            .ok_or_else(|| internal_error!("parent hook fallback"))?;
        let expected = usize::from(matches!(operation, ParentHook::Set));
        if arguments.len() != expected {
            return Err(NativeError::argument_count_error(format!(
                "{}::${name}::{}() expects exactly {expected} argument{}, {} given",
                parent.name,
                operation.name(),
                if expected == 1 { "" } else { "s" },
                arguments.len()
            )));
        }
        let key = self.property_key(&object, &name)?;
        let virtual_property = self.classes.for_object(&object).is_some_and(|class| {
            super::property_hooks::for_key(&class, &key)
                .and_then(|property| property.hooks.as_ref())
                .is_some_and(|hooks| hooks.hooks.is_virtual)
        });
        if virtual_property {
            let action = match operation {
                ParentHook::Get => "read from",
                ParentHook::Set => "write to",
            };
            return Err(property_error(
                &object,
                &name,
                &format!("Must not {action} virtual property "),
                "",
            ));
        }
        match operation {
            ParentHook::Get => {
                self.read_declared_property(&object, &name, crate::access_mode::Missing::Warn)
            }
            ParentHook::Set => {
                let Some(value) = arguments.into_iter().next() else {
                    return Err(internal_error!("parent setter argument"));
                };
                let mut path = crate::inline_stack::InlineStack::new();
                path.push(crate::path_update::PathKey::Property(key));
                self.write_detached_place(
                    Value::Object(Rc::clone(&object)),
                    &path,
                    value.clone(),
                    crate::access_mode::Mutation::Assign,
                )
                .await?;
                Ok(value)
            }
        }
    }
}
