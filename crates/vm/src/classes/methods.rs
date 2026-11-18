//! Method calls reuse argument routing and push user bodies onto the VM heap stack.

use std::rc::Rc;

use phera_bytecode::{NameId, Reg, Visibility};
use phera_extension_api::NativeError;
use phera_value::{Object, Value};

use super::{Class, Invocation, Method, MethodContext};
use crate::Vm;
use crate::calls::call_state::{CallTarget, Callee};

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
            .unit()
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
        let resolved = self.find_method(&class, name, Invocation::Method)?;
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
                .and_then(|frame| frame.function().instructions.get(frame.ip()));
            let usage = match instruction {
                Some(phera_bytecode::Instruction::MakeCallable { .. }) => {
                    crate::callables::CallableUse::FirstClass
                }
                _ => crate::callables::CallableUse::Callback,
            };
            let callee = self.resolve_callable_for(&Value::Object(receiver), usage)?;
            let mut callee = callee;
            callee.name = "Closure::__invoke".into();
            callee.argument_binding = crate::calls::callback_binding::ArgumentBinding::Internal;
            return self.prepare_call(callee);
        }
        let resolved = self.find_method(&class, name, invocation)?;
        let Some(super::method_lookup::ResolvedMethod { method, scope }) = resolved else {
            if invocation == Invocation::Constructor {
                let object = receiver.ok_or_else(|| internal_error!("constructor receiver"))?;
                self.prepare_constructed_object(object);
                return Ok(());
            }
            return Err(NativeError::error(format!(
                "Call to undefined method {}::{name}()",
                class.name
            )));
        };
        self.prepare_resolved_method(
            class,
            receiver,
            super::method_lookup::ResolvedMethod { method, scope },
            invocation,
        )
    }

    /// Bind a validated lookup to this call's receiver without repeating resolution.
    pub(crate) fn prepare_resolved_method(
        &mut self,
        class: Rc<Class>,
        receiver: Option<Rc<Object>>,
        resolved: super::method_lookup::ResolvedMethod,
        invocation: Invocation,
    ) -> Result<(), NativeError> {
        let super::method_lookup::ResolvedMethod { method, scope } = resolved;
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
        let signature = method.signature(&self.code);
        let call_name = signature.name.clone();
        let parameters = signature.parameters.clone();
        let context = MethodContext {
            receiver: if method.is_static { None } else { receiver },
            scope,
            called: class,
            invocation,
        };
        let callee = Callee {
            argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
            name: call_name,
            parameters,
            target: CallTarget::Method { method, context },
        };
        self.prepare_call(callee)
    }

    /// Visibility is checked against the lexical scope, never the receiver's class alone.
    pub(crate) fn check_method_visibility(&self, method: &Method) -> Result<(), NativeError> {
        let initializer_scope = self
            .initializer_scope
            .as_ref()
            .and_then(|name| self.classes.get(name));
        let scope = initializer_scope
            .as_ref()
            .or_else(|| self.current().and_then(|frame| frame.scope()));
        let allowed = match method.visibility {
            Visibility::Public => true,
            Visibility::Private => scope.is_some_and(|scope| scope.name == method.declaring),
            Visibility::Protected => scope.is_some_and(|scope| {
                scope.is_a(&method.declaring)
                    || self
                        .classes
                        .get(&method.declaring)
                        .is_some_and(|declaring| {
                            declaring.is_a(&scope.name)
                                || protected_prototype_applies(scope, declaring, method)
                        })
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
}

/// A protected override keeps the visibility of its ancestor's prototype.
fn protected_prototype_applies(scope: &Class, declaring: Rc<Class>, method: &Method) -> bool {
    let name = method.name.to_ascii_lowercase();
    let mut ancestor = declaring.parent.as_ref();
    while let Some(class) = ancestor {
        if scope.is_a(&class.name)
            && class
                .methods
                .get(&name)
                .is_some_and(|method| method.visibility == Visibility::Protected)
        {
            return true;
        }
        ancestor = class.parent.as_ref();
    }
    false
}

impl Vm {
    /// A missing constructor still consumes arguments and returns its allocated receiver.
    pub(crate) fn prepare_constructed_object(&mut self, object: Rc<Object>) {
        let parts = (&mut self.call_stack, &mut self.registers);
        crate::calls::php_windows::push_constructed(parts, object);
    }
}
