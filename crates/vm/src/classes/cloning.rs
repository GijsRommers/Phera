//! `clone($original, ['name' => $name])` copies the object, invokes __clone,
//! then applies the supplied property updates in order.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use phera_bytecode::Reg;
use phera_extension_api::{NativeClone, NativeError, NativeResult};
use phera_value::{ArrayKey, Object, PhpArray, Value};

use super::class::ClassOrigin;
use crate::Vm;
use crate::property_hook_continuation::HookContinuation;

/// PHP unlocks each readonly slot once for the hook, and again for clone-with.
#[derive(Default)]
pub(crate) struct CloneScopes(Rc<RefCell<HashMap<u32, Vec<ArrayKey>>>>);

pub(crate) struct CloneScope {
    object: u32,
    registry: Rc<RefCell<HashMap<u32, Vec<ArrayKey>>>>,
}

impl Drop for CloneScope {
    fn drop(&mut self) {
        self.registry.borrow_mut().remove(&self.object);
    }
}

pub(crate) struct CloneUpdates {
    object: Rc<Object>,
    properties: Rc<PhpArray>,
    next: Option<usize>,
    _scope: Option<CloneScope>,
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
        self.start_clone(destination, value, properties).await
    }

    pub(crate) async fn clone_intrinsic(&mut self, arguments: Vec<Value>) -> NativeResult {
        let (value, properties) = clone_arguments(arguments)?;
        if let Value::Object(object) = &value
            && object.lazy().is_some()
        {
            self.initialize_lazy_via_callback(object.clone()).await?;
        }
        let (object, properties) = self.copy_clone(value, properties).await?;
        let mut updates = self.clone_updates(object, properties);
        while let Some((name, value)) = updates.next()? {
            if let Some(setter) = self
                .plan_selected_property_write(updates.object.clone(), name, value)
                .await?
            {
                self.invoke_declared_hook(setter.function, setter.object, Some(setter.argument))
                    .await?;
            }
        }
        Ok(Value::Object(updates.object))
    }

    pub(crate) async fn suspend_clone_intrinsic(
        &mut self,
        destination: Reg,
        arguments: Vec<Value>,
    ) -> Result<(), NativeError> {
        let (value, properties) = clone_arguments(arguments)?;
        self.start_clone(destination, value, properties).await
    }

    pub(crate) async fn start_clone(
        &mut self,
        destination: Reg,
        value: Value,
        properties: Option<Value>,
    ) -> Result<(), NativeError> {
        if let Value::Object(original) = &value {
            let object = crate::lazy_objects::resolved_lazy_instance(original.clone());
            if object.is_uninitialized_lazy() {
                let continuation = crate::lazy_continuation::LazyContinuation::Clone(
                    destination,
                    original.clone(),
                    properties,
                );
                self.suspend_lazy_initialization(object, continuation)
                    .await?;
                return Ok(());
            }
        }
        let (object, properties) = self.copy_clone(value, properties).await?;
        self.continue_clone_updates(destination, self.clone_updates(object, properties))
            .await
    }

    async fn copy_clone(
        &mut self,
        value: Value,
        with: Option<Value>,
    ) -> Result<(Rc<Object>, Rc<PhpArray>), NativeError> {
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
        let mut original = original;
        let mut proxies = Vec::new();
        while let Some(instance) = original.lazy_instance() {
            let class = self
                .classes
                .for_object(&original)
                .ok_or_else(|| internal_error!("clone proxy class"))?;
            proxies.push(self.classes.allocate(class));
            original = instance;
        }
        let class = self
            .classes
            .for_object(&original)
            .ok_or_else(|| internal_error!("clone class"))?;
        let policy = class.hooks.as_ref().and_then(|hooks| hooks.clone);
        if class.kind == phera_bytecode::ClassKind::Enum
            || class.is_throwable
            || matches!(policy, Some(NativeClone::Forbidden))
        {
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
            return Ok((cloned, with));
        }
        let hook = class.magic.as_ref().and_then(|magic| magic.clone.clone());
        if let Some(method) = &hook {
            self.check_clone_visibility(method)?;
        }
        let cloned = self.classes.allocate(class.clone());
        match policy {
            Some(NativeClone::Copy(copy)) => copy(&original, &cloned)?,
            Some(NativeClone::CopyWithHandle(copy)) => copy(&original, &cloned)?,
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
            let _scope = self.begin_clone_updates(&cloned);
            self.invoke_magic_method(method, class.clone(), cloned.clone(), [])
                .await?;
        }
        let mut result = cloned;
        while let Some(proxy) = proxies.pop() {
            proxy.replace_lazy(Some(Box::new(phera_value::LazyObject::Proxy(result))));
            result = proxy;
        }
        Ok((result, with))
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

    fn begin_clone_updates(&self, object: &Object) -> CloneScope {
        self.clone_scopes
            .0
            .borrow_mut()
            .insert(object.id, Vec::new());
        CloneScope {
            object: object.id,
            registry: self.clone_scopes.0.clone(),
        }
    }

    pub(crate) fn clone_can_update(&self, object: &Object, key: &ArrayKey) -> bool {
        self.clone_scopes
            .0
            .borrow()
            .get(&object.id)
            .is_some_and(|changed| !changed.contains(key))
    }

    pub(crate) fn clone_updated(&mut self, object: &Object, key: &ArrayKey) {
        if let Some(changed) = self.clone_scopes.0.borrow_mut().get_mut(&object.id) {
            changed.push(key.clone());
        }
    }

    fn clone_updates(&self, object: Rc<Object>, properties: Rc<PhpArray>) -> CloneUpdates {
        CloneUpdates {
            next: properties.first_position(),
            _scope: (!properties.is_empty()).then(|| self.begin_clone_updates(&object)),
            object,
            properties,
        }
    }

    pub(crate) async fn continue_clone_updates(
        &mut self,
        destination: Reg,
        mut updates: CloneUpdates,
    ) -> Result<(), NativeError> {
        while let Some((name, value)) = updates.next()? {
            if let Some(setter) = self
                .plan_selected_property_write(updates.object.clone(), name, value)
                .await?
            {
                return self
                    .suspend_for_hook(
                        setter.function,
                        setter.object,
                        Some(setter.argument),
                        HookContinuation::Clone(destination, updates),
                    )
                    .await;
            }
        }
        self.set(destination, Value::Object(updates.object));
        Ok(())
    }
}

impl CloneUpdates {
    fn next(&mut self) -> Result<Option<(ArrayKey, Value)>, NativeError> {
        let Some(position) = self.next else {
            return Ok(None);
        };
        let (name, value) = self
            .properties
            .entry_at(position)
            .ok_or_else(|| internal_error!("clone property cursor"))?;
        self.next = self.properties.next_position(position);
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
        Ok(Some((ArrayKey::str(name), value.deref().clone())))
    }
}

fn clone_arguments(arguments: Vec<Value>) -> Result<(Value, Option<Value>), NativeError> {
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
    Ok((value, arguments.next()))
}
