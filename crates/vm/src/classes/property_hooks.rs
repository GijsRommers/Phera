//! `$object->name` calls the linked getter. The current hook frame can use
//! its own receiver and property prototype to bypass dispatch to backing storage.

use std::rc::Rc;

use phera_bytecode::{CompiledUnit, FunctionId, PropertyHook, PropertyHooks, Visibility};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Object, Value};

use crate::Vm;
use crate::calls::callback_binding::ArgumentOrigin;
use crate::calls::prepared_call::CallKind;
use crate::code_table::FunctionIndex;

use super::{Class, class::PropertyDefault};

#[derive(Clone)]
pub(crate) struct LinkedPropertyHooks {
    pub(crate) hooks: PropertyHooks<FunctionIndex>,
    pub(crate) prototype: Rc<()>,
}

impl LinkedPropertyHooks {
    pub(super) fn link(
        hooks: &PropertyHooks<FunctionId>,
        vm: &Vm,
        unit: &std::sync::Arc<CompiledUnit>,
    ) -> Self {
        let link = |hook: &PropertyHook<FunctionId>| PropertyHook {
            function: vm.code.index_in(unit, Some(hook.function.index())),
            is_abstract: hook.is_abstract,
            is_final: hook.is_final,
            attributes: hook.attributes.clone(),
        };
        Self {
            hooks: PropertyHooks {
                get: hooks.get.as_ref().map(link),
                set: hooks.set.as_ref().map(link),
                is_virtual: hooks.is_virtual,
                set_visibility: hooks.set_visibility,
            },
            prototype: Rc::new(()),
        }
    }

    pub(super) fn methods(&self) -> impl Iterator<Item = &PropertyHook<FunctionIndex>> {
        self.hooks.get.iter().chain(&self.hooks.set)
    }
}

/// Resolve the effective declaration by its visibility-checked storage key.
pub(crate) fn for_key<'a>(class: &'a Class, key: &ArrayKey) -> Option<&'a PropertyDefault> {
    class
        .defaults
        .iter()
        .find(|property| property.key == *key && property.hooks.is_some())
}

impl Vm {
    pub(crate) fn uses_property_backing(
        &self,
        object: &Object,
        property: &PropertyDefault,
        hooks: &LinkedPropertyHooks,
    ) -> bool {
        let Some(frame) = self.current() else {
            return false;
        };
        let Some((_, name)) = frame.function().name.split_once("::$") else {
            return false;
        };
        if !name
            .split_once("::")
            .is_some_and(|(name, _)| name == property.name)
            || !frame
                .this()
                .is_some_and(|receiver| std::ptr::eq(&**receiver, object))
        {
            return false;
        }
        frame.scope().is_some_and(|scope| {
            scope.defaults.iter().any(|candidate| {
                candidate.name == property.name
                    && candidate
                        .hooks
                        .as_ref()
                        .is_some_and(|own| Rc::ptr_eq(&own.prototype, &hooks.prototype))
            })
        })
    }

    /// A hook uses the ordinary method window, and never holds an object borrow.
    pub(crate) fn prepare_declared_hook(
        &mut self,
        function: FunctionIndex,
        object: Rc<Object>,
        value: Option<Value>,
    ) -> Result<(), NativeError> {
        crate::calls::php_windows::push_php_call(
            &self.code,
            (&mut self.call_stack, &mut self.registers),
            function,
            CallKind::Method,
            [Some(Value::Object(object))],
        );
        if let Some(value) = value {
            self.store_sent_value(0, value)?;
        }
        Ok(())
    }

    pub(crate) async fn invoke_declared_hook(
        &mut self,
        function: FunctionIndex,
        object: Rc<Object>,
        argument: Option<Value>,
    ) -> NativeResult {
        let base = self.registers.len();
        let depth = self.call_stack.call_depth();
        let routed = self.prepare_declared_hook(function, object, argument);
        self.finish_callback(routed, base, depth, ArgumentOrigin::Php)
            .await
    }

    pub(crate) fn hook_set_access(
        &self,
        property: &PropertyDefault,
        hooks: &LinkedPropertyHooks,
        operation: &str,
    ) -> Result<(), NativeError> {
        let Some(visibility) = hooks.hooks.set_visibility else {
            return Ok(());
        };
        let scope = self.current().and_then(|frame| frame.scope());
        let allowed = match visibility {
            Visibility::Public => true,
            Visibility::Private => scope.is_some_and(|scope| scope.name == property.declaring),
            Visibility::Protected => scope.is_some_and(|scope| {
                scope.is_a(&property.declaring)
                    || self
                        .classes
                        .get(&property.declaring)
                        .is_some_and(|declaring| declaring.is_a(&scope.name))
            }),
        };
        if allowed {
            return Ok(());
        }
        let visibility = match visibility {
            Visibility::Public => "public",
            Visibility::Protected => "protected",
            Visibility::Private => "private",
        };
        let scope = scope.map_or("global scope".into(), |class| {
            format!("scope {}", class.name)
        });
        Err(NativeError::error(format!(
            "Cannot {operation} {visibility}(set) property {}::${} from {scope}",
            phera_value::class_display_name(&property.declaring),
            property.name
        )))
    }
}

pub(crate) fn indirect_error(object: &Object, name: &str) -> NativeError {
    property_error(object, name, "Indirect modification of ", " is not allowed")
}

pub(crate) fn property_error(
    object: &Object,
    name: &str,
    before: &str,
    after: &str,
) -> NativeError {
    let class = phera_value::class_display_name(object.class.name());
    NativeError::error(format!("{before}{class}::${name}{after}"))
}

pub(crate) enum HookWrite {
    Storage,
    Setter(FunctionIndex),
}

impl Vm {
    pub(crate) async fn read_declared_hook(
        &mut self,
        object: &Rc<Object>,
        property: &PropertyDefault,
        hooks: &LinkedPropertyHooks,
        indirect: bool,
    ) -> Result<Option<Value>, NativeError> {
        let Some(getter) = &hooks.hooks.get else {
            if hooks.hooks.is_virtual {
                return Err(property_error(
                    object,
                    &property.name,
                    "Property ",
                    " is write-only",
                ));
            }
            if indirect {
                let value = self.read_declared_property(
                    object,
                    &property.name,
                    crate::access_mode::Missing::Warn,
                )?;
                if !value.is_object() {
                    return Err(indirect_error(object, &property.name));
                }
                return Ok(Some(value));
            }
            return Ok(None);
        };
        if self.uses_property_backing(object, property, hooks) {
            if hooks.hooks.is_virtual {
                return Err(property_error(
                    object,
                    &property.name,
                    "Must not read from virtual property ",
                    "",
                ));
            }
            return Ok(None);
        }
        if indirect {
            self.hook_set_access(property, hooks, "indirectly modify")?;
        }
        let value = self
            .invoke_declared_hook(getter.function, object.clone(), None)
            .await?;
        if indirect && !value.is_reference() && !value.is_object() {
            return Err(indirect_error(object, &property.name));
        }
        Ok(Some(value))
    }

    pub(crate) fn declared_hook_write(
        &self,
        object: &Object,
        property: &PropertyDefault,
        hooks: &LinkedPropertyHooks,
    ) -> Result<HookWrite, NativeError> {
        let Some(setter) = &hooks.hooks.set else {
            if hooks.hooks.is_virtual {
                return Err(property_error(
                    object,
                    &property.name,
                    "Property ",
                    " is read-only",
                ));
            }
            self.hook_set_access(property, hooks, "modify")?;
            return Ok(HookWrite::Storage);
        };
        if self.uses_property_backing(object, property, hooks) {
            if hooks.hooks.is_virtual {
                return Err(property_error(
                    object,
                    &property.name,
                    "Must not write to virtual property ",
                    "",
                ));
            }
            return Ok(HookWrite::Storage);
        }
        self.hook_set_access(property, hooks, "modify")?;
        Ok(HookWrite::Setter(setter.function))
    }
}
