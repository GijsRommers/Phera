//! Method calls reuse argument routing and push user bodies onto the VM heap stack.

use std::rc::Rc;

use phera_bytecode::{NameId, Reg, Visibility};
use phera_extension_api::{NativeError, NativeMethod, NativeResult};
use phera_value::{Object, Value};

use super::{Class, Invocation, Method, MethodBody, MethodContext};
use crate::Vm;
use crate::call_state::{CallTarget, Callee};

impl Vm {
    /// Receiver errors precede argument evaluation, matching ordinary PHP calls.
    pub(crate) fn init_method_call(
        &mut self,
        receiver: Reg,
        method: NameId,
    ) -> Result<(), NativeError> {
        let unit = self
            .current()
            .ok_or_else(|| internal_error!("method frame"))?
            .unit
            .clone();
        let name = unit
            .names
            .get(method.index())
            .ok_or_else(|| internal_error!("method name"))?;
        let value = self.read_local(receiver);
        let Value::Object(object) = value else {
            return Err(NativeError::error(format!(
                "Call to a member function {name}() on {}",
                value.type_name()
            )));
        };
        let class = self
            .classes
            .for_object(&object)
            .ok_or_else(|| internal_error!("receiver class"))?;
        let resolved = self.cached_method(&class, name, Invocation::Method)?;
        if let Some(resolved) = resolved {
            self.prepare_resolved_method(class, Some(object), resolved, Invocation::Method)?;
        } else {
            self.prepare_method(class, Some(object), name, Invocation::Method)?;
        }
        Ok(())
    }

    /// Snapshot a bound target so nested calls during argument sends cannot replace the receiver.
    pub(crate) fn prepare_method(
        &mut self,
        class: Rc<Class>,
        receiver: Option<Rc<Object>>,
        name: &str,
        invocation: Invocation,
    ) -> Result<(), NativeError> {
        if class.name == "Closure" && name.eq_ignore_ascii_case("__invoke") {
            let receiver = receiver.ok_or_else(|| {
                NativeError::error("Call to undefined method Closure::__invoke()")
            })?;
            let instruction = self
                .current()
                .and_then(|frame| frame.function().instructions.get(frame.ip));
            let usage = match instruction {
                Some(phera_bytecode::Instruction::MakeCallable { .. }) => {
                    crate::callables::CallableUse::FirstClass
                }
                _ => crate::callables::CallableUse::Callback,
            };
            let callee = self.resolve_callable_for(&Value::Object(receiver), usage)?;
            let mut callee = (*callee).clone();
            callee.name = "Closure::__invoke".into();
            callee.argument_binding = crate::callback_binding::ArgumentBinding::Internal;
            let callee = self.call_stack.bound_callee(callee);
            return self.prepare_call(callee);
        }
        let resolved = self.cached_method(&class, name, invocation)?;
        let Some(super::method_cache::ResolvedMethod { method, scope }) = resolved else {
            if invocation == Invocation::Constructor {
                let callee = self.call_stack.bound_callee(Callee {
                    argument_binding: crate::callback_binding::ArgumentBinding::Caller,
                    name: class.empty_constructor.name.clone(),
                    parameters: class.empty_constructor.parameters.clone(),
                    target: CallTarget::Constructed(
                        receiver.ok_or_else(|| internal_error!("constructor receiver"))?,
                    ),
                });
                return self.prepare_call(callee);
            }
            return Err(NativeError::error(format!(
                "Call to undefined method {}::{name}()",
                class.name
            )));
        };
        self.prepare_resolved_method(
            class,
            receiver,
            super::method_cache::ResolvedMethod { method, scope },
            invocation,
        )
    }

    /// Bind a validated lookup to this call's receiver without repeating resolution.
    pub(crate) fn prepare_resolved_method(
        &mut self,
        class: Rc<Class>,
        receiver: Option<Rc<Object>>,
        resolved: super::method_cache::ResolvedMethod,
        invocation: Invocation,
    ) -> Result<(), NativeError> {
        let super::method_cache::ResolvedMethod { method, scope } = resolved;
        if method.is_abstract {
            return Err(NativeError::error(format!(
                "Cannot call abstract method {}::{}()",
                method.declaring, method.name
            )));
        }
        if !method.is_static && receiver.is_none() {
            return Err(NativeError::error(format!(
                "Non-static method {}::{}() cannot be called statically",
                method.declaring, method.name
            )));
        }
        let signature = method.signature();
        let call_name = signature.name.clone();
        let parameters = signature.parameters.clone();
        let context = MethodContext {
            receiver: if method.is_static { None } else { receiver },
            scope,
            called: class,
            invocation,
        };
        let callee = self.call_stack.bound_callee(Callee {
            argument_binding: crate::callback_binding::ArgumentBinding::Caller,
            name: call_name,
            parameters,
            target: CallTarget::Method { method, context },
        });
        self.prepare_call(callee)
    }

    /// Visibility is checked against the lexical scope, never the receiver's class alone.
    pub(crate) fn check_method_visibility(&self, method: &Method) -> Result<(), NativeError> {
        let initializer_scope = self
            .initializer_scope
            .as_ref()
            .and_then(|name| self.classes.get(name));
        let scope = initializer_scope.as_ref().or_else(|| {
            self.current()
                .and_then(|frame| frame.method.as_ref())
                .map(|context| &context.scope)
        });
        let allowed = match method.visibility {
            Visibility::Public => true,
            Visibility::Private => scope.is_some_and(|scope| scope.name == method.declaring),
            Visibility::Protected => scope.is_some_and(|scope| {
                scope.is_a(&method.declaring)
                    || self
                        .classes
                        .get(&method.declaring)
                        .is_some_and(|declaring| declaring.is_a(&scope.name))
            }),
        };
        if allowed {
            return Ok(());
        }
        let visibility = if method.visibility == Visibility::Private {
            "private"
        } else {
            "protected"
        };
        let from = scope
            .map(|scope| format!("scope {}", scope.name))
            .unwrap_or_else(|| "global scope".into());
        Err(NativeError::error(format!(
            "Call to {visibility} method {}::{}() from {from}",
            method.declaring, method.name
        )))
    }

    /// User methods suspend the caller; native methods complete in the same dispatch step.
    pub(crate) async fn invoke_method(
        &mut self,
        method: &Method,
        context: MethodContext,
        destination: Reg,
        arguments: Vec<Option<Value>>,
        origin: crate::callback_binding::ArgumentOrigin,
    ) -> Result<(), NativeError> {
        match &method.body {
            MethodBody::Closure(operation) => {
                let arguments = arguments
                    .into_iter()
                    .map(Option::unwrap_or_default)
                    .collect();
                let value = Box::pin(self.closure_method_named(
                    *operation,
                    context.receiver,
                    arguments,
                    0,
                    Vec::new(),
                    origin,
                ))
                .await?;
                self.set(destination, value);
            }
            MethodBody::User(reference) => {
                let pushed = self.with_argument_origin(origin, |vm| {
                    vm.push_prepared(
                        &reference.unit,
                        reference.index,
                        Some(destination),
                        arguments,
                    )
                });
                if let Err(error) = pushed {
                    if self.current().is_some_and(|frame| {
                        frame.ip == 0
                            && frame.function_id == Some(reference.index)
                            && std::sync::Arc::ptr_eq(&frame.unit, &reference.unit)
                    }) {
                        let context = self.call_stack.method_context(context);
                        if let Some(frame) = self.call_stack.current_mut() {
                            frame.method = Some(context);
                        }
                    }
                    return Err(error);
                }
                let context = self.call_stack.method_context(context);
                if let Some(frame) = self.call_stack.current_mut() {
                    frame.method = Some(context);
                }
            }
            MethodBody::Native(entry) => {
                let mut arguments: Vec<Value> = arguments
                    .into_iter()
                    .map(Option::unwrap_or_default)
                    .collect();
                self.with_argument_origin(origin, |vm| {
                    vm.prepare_native_method_arguments(method, entry.params, &mut arguments)
                })?;
                let result: NativeResult = match entry.handler {
                    NativeMethod::Sync(handler) => {
                        handler(self, context.receiver.as_ref(), &arguments)
                    }
                    NativeMethod::Async(handler) => {
                        handler(self, context.receiver.clone(), arguments).await
                    }
                    NativeMethod::Abstract => {
                        Err(NativeError::error("Cannot call abstract method"))
                    }
                };
                let result = result?;
                let result = if matches!(method.declaring.as_str(), "Exception" | "Error") {
                    context
                        .receiver
                        .as_ref()
                        .and_then(|object| self.throwable_method_result(&method.name, object))
                        .unwrap_or(result)
                } else {
                    result
                };
                self.set(
                    destination,
                    if context.invocation == Invocation::Constructor {
                        context.receiver.map(Value::Object).unwrap_or_default()
                    } else {
                        result
                    },
                );
            }
        }
        Ok(())
    }
}

impl Vm {
    /// Mirrors prepare_method's missing-constructor branch using linked presence metadata.
    pub(crate) fn prepare_constructed_object(
        &mut self,
        class: &Class,
        object: Rc<Object>,
    ) -> Result<(), NativeError> {
        let callee = self.call_stack.bound_callee(Callee {
            argument_binding: crate::callback_binding::ArgumentBinding::Caller,
            name: class.empty_constructor.name.clone(),
            parameters: class.empty_constructor.parameters.clone(),
            target: CallTarget::Constructed(object),
        });
        self.prepare_call(callee)
    }
}
