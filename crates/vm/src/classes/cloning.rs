//! `clone($original, ['name' => $name])` copies the object, invokes __clone,
//! then applies the supplied property updates in order.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Object, PhpArray, Value};

use crate::Vm;
use crate::access_mode::Mutation;
use crate::inline_stack::InlineStack;
use crate::path_update::PathKey;

use super::class::ClassOrigin;

/// PHP unlocks each readonly slot once for the hook, and again for clone-with.
pub(crate) struct CloneScope {
    object: u32,
    changed: Vec<ArrayKey>,
}

impl Vm {
    pub(crate) async fn clone_instruction(
        &mut self,
        destination: Reg,
        source: Reg,
        with: Option<Reg>,
    ) -> Result<(), NativeError> {
        let value = self.read_local(source);
        let properties = with.map(|with| self.read_local(with));
        let cloned = self.clone_value(value, properties).await?;
        self.set(destination, cloned);
        Ok(())
    }

    pub(crate) async fn clone_intrinsic(&mut self, arguments: Vec<Value>) -> NativeResult {
        if !(1..=2).contains(&arguments.len()) {
            let bound = if arguments.is_empty() {
                "at least 1"
            } else {
                "at most 2"
            };
            return Err(NativeError::argument_count_error(format!(
                "clone() expects {bound} argument{}, {} given",
                if arguments.is_empty() { "" } else { "s" },
                arguments.len()
            )));
        }
        let mut arguments = arguments.into_iter();
        let value = arguments.next().unwrap_or_default();
        self.clone_value(value, arguments.next()).await
    }

    async fn clone_value(&mut self, value: Value, with: Option<Value>) -> NativeResult {
        let Value::Object(original) = value.deref().clone() else {
            return Err(NativeError::type_error(format!(
                "clone(): Argument #1 ($object) must be of type object, {} given",
                value.type_name()
            )));
        };
        let with = match with {
            Some(Value::Array(properties)) => properties,
            Some(other) => {
                return Err(NativeError::type_error(format!(
                    "clone(): Argument #2 ($withProperties) must be of type array, {} given",
                    other.type_name()
                )));
            }
            None => Rc::new(PhpArray::new()),
        };
        let class = self
            .classes
            .for_object(&original)
            .ok_or_else(|| internal_error!("clone class"))?;
        if class.is_throwable {
            return Err(NativeError::error(format!(
                "Trying to clone an uncloneable object of class {}",
                class.name
            )));
        }
        if class.origin == ClassOrigin::Closure {
            if !with.is_empty() {
                return Err(NativeError::error(
                    "Cloning objects of class Closure with updated properties is not supported",
                ));
            }
            let state = original
                .native
                .borrow()
                .as_ref()
                .and_then(|state| state.downcast_ref::<crate::closures::ClosureState>())
                .map(|state| state.duplicate())
                .ok_or_else(|| internal_error!("clone closure"))?;
            return self.allocate_closure(state).map(Value::Object);
        }
        let hook = class.magic.as_ref().and_then(|magic| magic.clone.clone());
        if let Some(method) = &hook {
            self.check_method_visibility(method)?;
        }
        let cloned = self.classes.allocate(class.clone());
        cloned.update(|properties| *properties = original.to_array());
        self.copy_unset_properties(&original, &cloned);
        self.track_object_property_references(&cloned)?;
        if let Some(method) = hook {
            self.begin_clone_updates(&cloned);
            let result = self
                .invoke_magic_method(method, class.clone(), cloned.clone(), [])
                .await;
            self.clone_scopes.pop();
            result?;
        }
        if !with.is_empty() {
            self.begin_clone_updates(&cloned);
            let result = self.update_clone_properties(&cloned, &with).await;
            self.clone_scopes.pop();
            result?;
        }
        Ok(Value::Object(cloned))
    }

    fn begin_clone_updates(&mut self, object: &Object) {
        self.clone_scopes.push(CloneScope {
            object: object.id,
            changed: Vec::new(),
        });
    }

    pub(crate) fn clone_can_update(&self, object: &Object, key: &ArrayKey) -> bool {
        self.clone_scopes
            .iter()
            .rev()
            .find(|scope| scope.object == object.id)
            .is_some_and(|scope| !scope.changed.contains(key))
    }

    pub(crate) fn clone_updated(&mut self, object: &Object, key: &ArrayKey) {
        if let Some(scope) = self
            .clone_scopes
            .iter_mut()
            .rev()
            .find(|scope| scope.object == object.id)
        {
            scope.changed.push(key.clone());
        }
    }

    async fn update_clone_properties(
        &mut self,
        object: &Rc<Object>,
        properties: &PhpArray,
    ) -> Result<(), NativeError> {
        for (name, value) in properties.iter() {
            if let Value::Reference(cell) = value
                && Rc::strong_count(cell) > 1
            {
                return Err(NativeError::error(
                    "Cannot assign by reference when cloning with updated properties",
                ));
            }
            let name = match name {
                ArrayKey::Int(index) => index.to_string(),
                ArrayKey::String(name) => name.to_string_lossy(),
            };
            if name.starts_with('\0') {
                return Err(NativeError::error(
                    "Cannot access property starting with \"\\0\"",
                ));
            }
            let class = self
                .classes
                .for_object(object)
                .ok_or_else(|| internal_error!("clone properties class"))?;
            let hook = crate::magic_properties::PropertyHook::Set;
            let written = ArrayKey::str(name.as_str());
            let (stored, slot) = self.property_slot(object, &name);
            if stored.is_none()
                && !slot
                    .as_ref()
                    .is_some_and(|slot| self.typed_property_uninitialized(&class, object, slot))
                && let Some(method) = self.property_hook(&class, object, &written, hook)
            {
                self.invoke_property_hook(
                    class,
                    object.clone(),
                    hook,
                    written,
                    method,
                    Some(value.deref().clone()),
                )
                .await?;
                continue;
            }
            let key = self.property_key(object, &name)?;
            let mut keys = InlineStack::new();
            keys.push(PathKey::Property(key));
            let mut value = value.deref().clone();
            let mut root = Value::Object(object.clone());
            self.validate_property_mutation(root.clone(), &keys, &mut value, Mutation::Assign)?;
            let value = self.check_path_reference_assignment(&root, &keys, value)?;
            let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
            crate::path_update::update(
                &mut root,
                &keys,
                value,
                Mutation::Assign,
                &mut diagnostics.buffer,
                &diagnostics.vm.property_references,
            )?;
        }
        Ok(())
    }
}
