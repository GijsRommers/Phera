//! Every evaluated callback uses one resolver before argument binding or native dispatch.

use crate::{Vm, calls::call_state::Callee, classes::Invocation};
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
    pub(crate) fn resolve_callable(&mut self, value: &Value) -> Result<Callee, NativeError> {
        self.resolve_callable_for(value, CallableUse::Callback)
    }

    /// Calling syntax selects the visibility and legacy callback rules before argument evaluation.
    pub(crate) fn resolve_callable_for(
        &mut self,
        value: &Value,
        usage: CallableUse,
    ) -> Result<Callee, NativeError> {
        match &*value.deref() {
            Value::Object(object) => self.resolve_callable_object(object, usage),
            Value::String(name) => {
                if let Some(callee) = self.function_table.resolved(name) {
                    return Ok(callee);
                }
                let written = String::from_utf8_lossy(name.as_bytes());
                if let Some((class, method)) = written.split_once("::") {
                    self.prepare_callable_class(class, method, usage)?;
                    self.take_resolved_callable()
                } else {
                    let callee = self.resolve_callee(&written)?;
                    self.function_table.cache_resolution(name, &callee);
                    Ok(callee)
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
    ) -> Result<Callee, NativeError> {
        if object.class.name() == "Closure" {
            let native = object.native.borrow();
            let state = native
                .as_ref()
                .and_then(|state| state.downcast_ref::<crate::closures::ClosureState>())
                .ok_or_else(|| internal_error!("closure callable"))?;
            let bound_function = state.context.is_some()
                && matches!(
                    state.callee.target,
                    crate::calls::call_state::CallTarget::User { .. }
                );
            if state.origin == crate::closures::ClosureOrigin::Callable
                && usage != CallableUse::FirstClass
                && !bound_function
            {
                return Ok(state.callee.clone());
            }
            Ok(Callee {
                argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
                name: state.callee.name.clone(),
                parameters: state.callee.parameters.clone(),
                target: crate::calls::call_state::CallTarget::Closure(object.clone()),
            })
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
            let resolved = crate::classes::method_lookup::ResolvedMethod { method, scope };
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
    ) -> Result<Callee, NativeError> {
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
        let method = String::from_utf8_lossy(method.as_bytes());
        match &*target.deref() {
            Value::String(class) => {
                self.prepare_callable_class(
                    &String::from_utf8_lossy(class.as_bytes()),
                    &method,
                    usage,
                )?;
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

    /// Take back the call that resolving a method prepared, as a callee.
    pub(crate) fn take_resolved_callable(&mut self) -> Result<Callee, NativeError> {
        self.take_prepared_callee()
    }

    /// Abandon the innermost prepared call and return its callee: `f(...)`
    /// makes a closure of it instead of calling it.
    pub(crate) fn take_prepared_callee(&mut self) -> Result<Callee, NativeError> {
        let (call, details) = self.call_stack.pop_call()?;
        self.registers.truncate(call.base());
        match (call.callee, details.and_then(|details| details.callee)) {
            (crate::calls::prepared_call::PreparedCallee::Php(function), _) => {
                Ok(self.function_callee(function))
            }
            (_, Some(callee)) => Ok(callee),
            (_, None) => Err(internal_error!("prepared callee")),
        }
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
