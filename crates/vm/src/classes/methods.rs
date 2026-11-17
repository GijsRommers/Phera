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
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("method frame"))?;
        let name = self.code.loaded(frame.function).names[method.index()];
        let value = self.read_local(receiver);
        let Value::Object(object) = value else {
            return Err(NativeError::error(format!(
                "Call to a member function {}() on {}",
                self.name(method)?,
                value.type_name()
            )));
        };
        let class = self
            .classes
            .index_for_object(&object)
            .ok_or_else(|| internal_error!("receiver class"))?;
        let resolved = self.resolve_method(class, name, Invocation::Method)?;
        if let Some(resolved) = resolved {
            self.prepare_resolved_method(class, Some(object), resolved, Invocation::Method)?;
        } else {
            let name = self.name(method)?;
            self.prepare_method(
                self.classes.at(class).clone(),
                Some(object),
                &name,
                Invocation::Method,
            )?;
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
                .and_then(|frame| frame.function(&self.code).instructions.get(frame.ip));
            let usage = match instruction {
                Some(phera_bytecode::Instruction::MakeCallable { .. }) => {
                    crate::callables::CallableUse::FirstClass
                }
                _ => crate::callables::CallableUse::Callback,
            };
            let callee = self.resolve_callable_for(&Value::Object(receiver), usage)?;
            let mut callee = callee;
            callee.signature = self.signatures.get(callee.signature).closure_invoke;
            callee.argument_binding = crate::calls::callback_binding::ArgumentBinding::Internal;
            return self.prepare_call(callee);
        }
        let resolved = self.resolve_named_method(class.index, name, invocation)?;
        let Some(super::method_lookup::ResolvedMethod { method, scope }) = resolved else {
            if invocation == Invocation::Constructor {
                return self.prepare_constructed_object(
                    &class,
                    receiver.ok_or_else(|| internal_error!("constructor receiver"))?,
                );
            }
            return Err(NativeError::error(format!(
                "Call to undefined method {}::{name}()",
                class.name
            )));
        };
        self.prepare_resolved_method(
            class.index,
            receiver,
            super::method_lookup::ResolvedMethod { method, scope },
            invocation,
        )
    }

    /// Bind a validated lookup to this call's receiver without repeating resolution.
    pub(crate) fn prepare_resolved_method(
        &mut self,
        class: super::class_table::ClassIndex,
        receiver: Option<Rc<Object>>,
        resolved: super::method_lookup::ResolvedMethod,
        invocation: Invocation,
    ) -> Result<(), NativeError> {
        let super::method_lookup::ResolvedMethod { method, scope } = resolved;
        let address = method;
        let method = self.classes.at(address.class).methods.at(address.slot);
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
        let signature = method.signature;
        let context = MethodContext {
            receiver: if method.is_static { None } else { receiver },
            scope,
            called: class,
            invocation,
        };
        let callee = Callee {
            argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
            signature,
            target: CallTarget::Method {
                method: address,
                context,
            },
        };
        self.prepare_call(callee)
    }

    /// Visibility is checked against the lexical scope, never the receiver's class alone.
    pub(crate) fn check_method_visibility(&self, method: &Method) -> Result<(), NativeError> {
        if method.visibility == Visibility::Public {
            return Ok(());
        }
        let initializer_scope = self
            .initializer_scope
            .as_ref()
            .and_then(|name| self.classes.get(name));
        let scope = initializer_scope.as_ref().or_else(|| {
            self.current()
                .and_then(|frame| self.call_stack.extras[frame.depth].method.as_ref())
                .map(|context| self.classes.at(context.scope))
        });
        let allowed = match method.visibility {
            Visibility::Public => true,
            Visibility::Private => scope.is_some_and(|scope| scope.index == method.declaring_index),
            Visibility::Protected => scope.is_some_and(|scope| {
                scope.includes(method.declaring_index)
                    || self
                        .classes
                        .at(method.declaring_index)
                        .includes(scope.index)
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

impl Vm {
    /// A missing constructor still consumes arguments and returns its allocated receiver.
    pub(crate) fn prepare_constructed_object(
        &mut self,
        class: &Class,
        object: Rc<Object>,
    ) -> Result<(), NativeError> {
        let callee = Callee {
            argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
            signature: class.empty_constructor,
            target: CallTarget::Constructed(object),
        };
        self.prepare_call(callee)
    }
}
