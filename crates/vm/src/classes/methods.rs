//! Method calls reuse argument routing and push user bodies onto the VM heap stack.

use std::rc::Rc;

use phera_bytecode::{NameId, Reg, Visibility};
use phera_extension_api::{NativeError, NativeMethod, NativeResult, prepare_call};
use phera_value::{Object, Value};

use super::{Class, Invocation, Method, MethodBody, MethodContext};
use crate::Vm;
use crate::call_state::{CallTarget, Callee, ParameterSpec};

impl Vm {
    /// Receiver errors precede argument evaluation, matching ordinary PHP calls.
    pub(crate) fn init_method_call(
        &mut self,
        receiver: Reg,
        method: NameId,
    ) -> Result<(), NativeError> {
        let name = self
            .current()
            .and_then(|frame| frame.unit.names.get(method.index()))
            .cloned()
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
            .get(object.class.name())
            .ok_or_else(|| internal_error!("receiver class"))?;
        self.prepare_method(class, Some(object), &name, Invocation::Method)
    }

    /// Snapshot a bound target so nested calls during argument sends cannot replace the receiver.
    pub(crate) fn prepare_method(
        &mut self,
        class: Rc<Class>,
        receiver: Option<Rc<Object>>,
        name: &str,
        invocation: Invocation,
    ) -> Result<(), NativeError> {
        let lexical_private = self
            .current()
            .and_then(|frame| frame.method.as_ref())
            .filter(|context| invocation == Invocation::Method && class.is_a(&context.scope.name))
            .and_then(|context| context.scope.methods.get(&name.to_ascii_lowercase()))
            .filter(|method| method.visibility == Visibility::Private)
            .cloned();
        let method =
            lexical_private.or_else(|| class.methods.get(&name.to_ascii_lowercase()).cloned());
        let Some(method) = method else {
            if invocation == Invocation::Constructor {
                return self.prepare_call(Rc::new(Callee {
                    name: format!("{}::__construct", class.name),
                    parameters: Vec::new(),
                    target: CallTarget::Constructed(
                        receiver.ok_or_else(|| internal_error!("constructor receiver"))?,
                    ),
                }));
            }
            return Err(NativeError::error(format!(
                "Call to undefined method {}::{name}()",
                class.name
            )));
        };
        self.check_method_visibility(&method)?;
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
        let scope = self
            .classes
            .get(&method.declaring)
            .ok_or_else(|| internal_error!("method scope"))?;
        let parameters = match &method.body {
            MethodBody::User(reference) => reference.unit.functions[reference.index]
                .parameters
                .iter()
                .map(|parameter| ParameterSpec {
                    name: reference.unit.names[parameter.name.index()].clone(),
                    by_reference: parameter.by_reference,
                    variadic: parameter.variadic,
                })
                .collect(),
            MethodBody::Native(entry) => entry
                .params
                .iter()
                .map(|parameter| ParameterSpec {
                    name: parameter.name.into(),
                    by_reference: parameter.by_ref,
                    variadic: parameter.variadic,
                })
                .collect(),
        };
        let context = MethodContext {
            receiver: if method.is_static { None } else { receiver },
            scope,
            called: class,
            invocation,
        };
        self.prepare_call(Rc::new(Callee {
            name: format!("{}::{}", method.declaring, method.name),
            parameters,
            target: CallTarget::Method { method, context },
        }))
    }

    /// Visibility is checked against the lexical scope, never the receiver's class alone.
    pub(crate) fn check_method_visibility(&self, method: &Method) -> Result<(), NativeError> {
        let scope = self
            .current()
            .and_then(|frame| frame.method.as_ref())
            .map(|context| &context.scope);
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
    ) -> Result<(), NativeError> {
        match &method.body {
            MethodBody::User(reference) => {
                self.push_prepared(
                    reference.unit.clone(),
                    reference.index,
                    Some(destination),
                    arguments,
                )?;
                if let Some(frame) = self.call_stack.current_mut() {
                    frame.method = Some(Box::new(context));
                }
            }
            MethodBody::Native(entry) => {
                let mut arguments: Vec<Value> = arguments
                    .into_iter()
                    .map(Option::unwrap_or_default)
                    .collect();
                prepare_call(
                    &format!("{}::{}", method.declaring, method.name),
                    entry.params,
                    &mut arguments,
                    self,
                )?;
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
