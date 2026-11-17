//! Every evaluated callback uses one resolver before argument binding or native dispatch.

use crate::{Vm, call_state::Callee, classes::Invocation};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Value};
use std::rc::Rc;

/// Direct invocation rejects legacy relative method names that callback APIs still accept.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallableUse {
    Direct,
    FirstClass,
    Callback,
}

impl Vm {
    /// Strings name functions or static methods; arrays bind an object or name a class.
    pub(crate) fn resolve_callable(&mut self, value: &Value) -> Result<Rc<Callee>, NativeError> {
        self.resolve_callable_for(value, CallableUse::Callback)
    }

    pub(crate) fn resolve_callable_for(
        &mut self,
        value: &Value,
        usage: CallableUse,
    ) -> Result<Rc<Callee>, NativeError> {
        match &*value.deref() {
            Value::Object(object) => self.resolve_callable_object(object, usage),
            Value::String(name) => {
                let name = name.to_string_lossy();
                if let Some((class, method)) = name.split_once("::") {
                    self.prepare_callable_class(class, method, usage)?;
                    self.take_resolved_callable()
                } else {
                    self.resolve_callee(name)
                }
            }
            Value::Array(array) => self.resolve_callable_array(array, usage),
            _ => Err(NativeError::error(format!(
                "Value of type {} is not callable",
                value.type_name()
            ))),
        }
    }

    /// Closures reuse their saved visibility; __invoke is PHP's public callable entry point.
    fn resolve_callable_object(
        &mut self,
        object: &Rc<phera_value::Object>,
        usage: CallableUse,
    ) -> Result<Rc<Callee>, NativeError> {
        if object.class.name() == "Closure" {
            let native = object.native.borrow();
            let state = native
                .as_ref()
                .and_then(|state| state.downcast_ref::<crate::closures::ClosureState>())
                .ok_or_else(|| internal_error!("closure callable"))?;
            let bound_function = state.context.is_some()
                && matches!(state.callee.target, crate::call_state::CallTarget::User(_));
            if state.origin == crate::closures::ClosureOrigin::Callable
                && usage != CallableUse::FirstClass
                && !bound_function
            {
                return Ok(state.callee.clone());
            }
            Ok(self.call_stack.bound_callee(Callee {
                argument_binding: crate::callback_binding::ArgumentBinding::Caller,
                name: state.callee.name.clone(),
                parameters: state.callee.parameters.clone(),
                target: crate::call_state::CallTarget::Closure(object.clone()),
            }))
        } else {
            let class = self
                .classes
                .for_object(object)
                .ok_or_else(|| internal_error!("invokable class"))?;
            let method = class.methods.get("__invoke").cloned().ok_or_else(|| {
                NativeError::error(format!("Object of type {} is not callable", class.name))
            })?;
            let scope = self
                .classes
                .get(&method.declaring)
                .ok_or_else(|| internal_error!("invokable scope"))?;
            let resolved = crate::classes::method_cache::ResolvedMethod {
                method,
                scope,
                result_plan: None,
            };
            self.prepare_resolved_method(
                class,
                Some(object.clone()),
                resolved,
                Invocation::Method,
            )?;
            self.take_resolved_callable()
        }
    }

    /// Array structure errors precede method lookup, regardless of the callback API.
    fn resolve_callable_array(
        &mut self,
        array: &phera_value::PhpArray,
        usage: CallableUse,
    ) -> Result<Rc<Callee>, NativeError> {
        if array.len() != 2 {
            return Err(NativeError::error(
                "Array callback must have exactly two elements",
            ));
        }
        let target = array
            .get(&ArrayKey::Int(0))
            .ok_or_else(|| NativeError::error("Array callback has to contain indices 0 and 1"))?;
        let method = array
            .get(&ArrayKey::Int(1))
            .ok_or_else(|| NativeError::error("Array callback has to contain indices 0 and 1"))?;
        let Value::String(method) = &*method.deref() else {
            return Err(NativeError::error(
                "Second array member is not a valid method",
            ));
        };
        let method = method.to_string_lossy();
        match &*target.deref() {
            Value::String(class) => {
                self.prepare_callable_class(&class.to_string_lossy(), &method, usage)?;
            }
            Value::Object(object) => {
                let class = self
                    .classes
                    .get(object.class.name())
                    .ok_or_else(|| internal_error!("callback class"))?;
                self.prepare_callback_method(class, Some(object.clone()), &method, usage)?;
            }
            _ => {
                return Err(NativeError::error(
                    "First array member is not a valid class name or object",
                ));
            }
        }
        self.take_resolved_callable()
    }

    /// Method preparation snapshots its scope in the existing pending-call representation.
    pub(crate) fn take_resolved_callable(&mut self) -> Result<Rc<Callee>, NativeError> {
        self.call_stack
            .current_mut()
            .and_then(|frame| frame.calls.pop())
            .map(|call| call.callee)
            .ok_or_else(|| internal_error!("resolved callable"))
    }
}

impl Vm {
    /// PHP's callback APIs retain deprecated parent-qualified array callbacks in 8.5.
    pub(crate) fn prepare_callback_method(
        &mut self,
        class: Rc<crate::classes::Class>,
        receiver: Option<Rc<phera_value::Object>>,
        name: &str,
        usage: CallableUse,
    ) -> Result<(), NativeError> {
        if receiver.is_none()
            && usage == CallableUse::Callback
            && let Some(method) = class.methods.get(&name.to_ascii_lowercase())
            && !method.is_static
        {
            return Err(NativeError::error(format!(
                "Non-static method {}::{}() cannot be called statically",
                method.declaring, method.name
            )));
        }
        let Some((qualifier, method)) = name.split_once("::") else {
            return self.prepare_method(class, receiver, name, Invocation::Method);
        };
        if usage != CallableUse::Callback {
            return self.prepare_method(class, receiver, name, Invocation::Method);
        }
        let target = match qualifier {
            "parent" => class.parent.clone(),
            "self" => Some(class.clone()),
            _ => self
                .classes
                .get(qualifier)
                .filter(|parent| class.is_a(&parent.name)),
        };
        let Some(target) = target else {
            return self.prepare_method(class, receiver, name, Invocation::Method);
        };
        self.diagnostic(
            "Deprecated",
            format!(
                "Callables of the form [\"{}\", \"{name}\"] are deprecated",
                class.name
            ),
        );
        self.prepare_method(target, receiver, method, Invocation::Method)?;
        self.forward_callback_class(class)
    }
}
