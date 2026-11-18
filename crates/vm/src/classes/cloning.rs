//! `clone($original, ['name' => $name])` copies the object, invokes __clone,
//! then applies the supplied property updates in order.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::{NativeClone, NativeError, NativeResult};
use phera_value::{ArrayKey, Object, PhpArray, Value};

use super::class::ClassOrigin;
use crate::Vm;

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
        let policy = class.hooks.as_ref().and_then(|hooks| hooks.clone);
        if class.is_throwable || matches!(policy, Some(NativeClone::Forbidden)) {
            return Err(NativeError::error(format!(
                "Trying to clone an uncloneable object of class {}",
                class.name
            )));
        }
        if class.origin == ClassOrigin::Closure {
            let state = original
                .native
                .borrow()
                .as_ref()
                .and_then(|state| state.downcast_ref::<crate::closures::ClosureState>())
                .map(|state| state.duplicate())
                .ok_or_else(|| internal_error!("clone closure"))?;
            let cloned = self.allocate_closure(state)?;
            self.update_clone_properties(&cloned, &with).await?;
            return Ok(Value::Object(cloned));
        }
        let hook = class.magic.as_ref().and_then(|magic| magic.clone.clone());
        if let Some(method) = &hook {
            self.check_clone_visibility(method)?;
        }
        let cloned = self.classes.allocate(class.clone());
        match policy {
            Some(NativeClone::Copy(copy)) => copy(&original, &cloned)?,
            None if class.has_native_layout
                && crate::reference_holders::has_extension_state(&original)? =>
            {
                return Err(NativeError::error(format!(
                    "Trying to clone an uncloneable object of class {}",
                    class.name
                )));
            }
            _ => {}
        }
        cloned.update(|properties| {
            let mut copied = original.to_array();
            for (_, value) in copied.iter_mut() {
                // The two holders are the old and new slots. With no PHP alias,
                // assigning the new slot must leave the old property alone.
                if let Value::Reference(cell) = value
                    && Rc::strong_count(cell) == 2
                {
                    let detached = cell.borrow().clone();
                    *value = detached;
                }
            }
            *properties = copied;
        });
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

    fn check_clone_visibility(&self, method: &super::Method) -> Result<(), NativeError> {
        let result = self.check_method_visibility(method);
        if result.is_ok() || method.visibility != phera_bytecode::Visibility::Protected {
            return result;
        }
        let caller = self.current().and_then(|frame| frame.scope());
        let mut class = self
            .classes
            .get(&method.declaring)
            .and_then(|class| class.parent.clone());
        while let Some(ancestor) = class {
            let Some(prototype) = ancestor
                .magic
                .as_ref()
                .and_then(|magic| magic.clone.as_ref())
            else {
                break;
            };
            if prototype.visibility == phera_bytecode::Visibility::Private {
                break;
            }
            if caller.is_some_and(|caller| caller.is_a(&prototype.declaring)) {
                return Ok(());
            }
            class = ancestor.parent.clone();
        }
        result
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
            self.assign_selected_property(
                object.clone(),
                ArrayKey::str(name),
                value.deref().clone(),
            )
            .await?;
        }
        Ok(())
    }
}
