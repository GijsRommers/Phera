//! A retained property receiver can be written after a compound read or clone.
//! `$proxy->child->count++` keeps the object returned by `__get('child')`.

use std::rc::Rc;

use phera_extension_api::{NativeError, NativePropertyMutation};
use phera_value::{ArrayKey, Object, Value};

use crate::Vm;
use crate::access_mode::Mutation;
use crate::code_table::FunctionIndex;
use crate::inline_stack::InlineStack;
use crate::magic_properties::PropertyHook;
use crate::path_update::{PathKey, property_name};

pub(crate) struct SelectedSetter {
    pub(crate) function: FunctionIndex,
    pub(crate) object: Rc<Object>,
    pub(crate) argument: Value,
}

impl Vm {
    pub(crate) async fn plan_selected_property_write(
        &mut self,
        object: Rc<Object>,
        name: ArrayKey,
        value: Value,
    ) -> Result<Option<SelectedSetter>, NativeError> {
        let object = crate::lazy_objects::resolved_lazy_instance(object);
        let class = self
            .classes
            .for_object(&object)
            .ok_or_else(|| internal_error!("property class"))?;
        let display = property_name(&name);
        if let Some(access) = class.property_access
            && (access.mutate)(
                self,
                &object,
                &display,
                NativePropertyMutation::Assign(&value),
            )?
        {
            return Ok(None);
        }
        let (stored, slot) = self.property_slot(&object, &display);
        let declared = slot
            .as_ref()
            .and_then(|key| crate::classes::property_hooks::for_key(&class, key));
        if let Some(property) = declared
            && let Some(hooks) = &property.hooks
            && let crate::classes::property_hooks::HookWrite::Setter(setter) =
                self.declared_hook_write(&object, property, hooks)?
        {
            return Ok(Some(SelectedSetter {
                function: setter,
                object,
                argument: value,
            }));
        }
        if declared.is_none()
            && stored.is_none()
            && !slot
                .as_ref()
                .is_some_and(|slot| self.typed_property_uninitialized(&class, &object, slot))
            && let Some(method) = self.property_hook(&class, &object, &name, PropertyHook::Set)
        {
            self.invoke_property_hook(class, object, PropertyHook::Set, name, method, Some(value))
                .await?;
            return Ok(None);
        }
        let key = match slot {
            Some(key) => key,
            None => self.property_key(&object, &display)?,
        };
        let mut keys = InlineStack::new();
        keys.push(PathKey::Property(key));
        self.write_detached_place(Value::Object(object), &keys, value, Mutation::Assign)
            .await
            .map(|_| None)
    }

    pub(crate) async fn write_detached_place(
        &mut self,
        mut root: Value,
        keys: &InlineStack<PathKey>,
        mut value: Value,
        mutation: Mutation,
    ) -> Result<Option<Value>, NativeError> {
        let mut converted = None;
        if mutation == Mutation::Assign
            && let Value::Object(object) = root.deref().clone()
            && let Some(PathKey::Property(key)) = keys.iter().next()
            && keys.iter().nth(1).is_none()
        {
            let object_value = value.is_object();
            value = self.property_string(&object, key, value).await?;
            if object_value && value.is_string() {
                converted = Some(value.clone());
            }
        }
        if self.validate_property_mutation(root.deref().clone(), keys, &mut value, mutation)? {
            return Ok(converted);
        }
        if mutation == Mutation::Assign {
            value = self.check_path_reference_assignment(&root, keys, value)?;
        }
        if mutation == Mutation::Unset
            && matches!(keys.iter().next(), Some(PathKey::Index(_)))
            && !root.is_array()
        {
            if root.is_null() {
                return Ok(None);
            }
            return Err(NativeError::error(
                "Cannot unset offset in a non-array variable",
            ));
        }
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        crate::path_update::update(
            &mut root,
            keys,
            value,
            mutation,
            &mut diagnostics.buffer,
            &diagnostics.vm.property_references,
        )?;
        Ok(converted)
    }

    pub(crate) fn warn_indirect_magic_property(
        &mut self,
        object: &phera_value::Object,
        name: &str,
        value: &Value,
    ) {
        if !value.is_reference() && !value.is_object() {
            self.diagnostic(
                "Notice",
                format!(
                    "Indirect modification of overloaded property {}::${name} has no effect",
                    object.class.name()
                ),
            );
        }
    }
}
