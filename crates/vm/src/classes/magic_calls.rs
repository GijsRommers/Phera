//! `parent::missing()` prefers __call on the current receiver and can also
//! use the parent's __callStatic if there is no eligible instance handler.

use std::rc::Rc;

use phera_bytecode::ClassKind;
use phera_extension_api::NativeError;
use phera_value::Object;

use super::method_lookup::ResolvedMethod;
use super::{Class, Invocation, MethodContext};
use crate::Vm;
use crate::calls::call_state::{CallTarget, Callee, ParameterSpec};
use crate::calls::callback_binding::ArgumentBinding;
use crate::calls::magic_dispatch::MagicCall;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MethodSyntax {
    Instance,
    Static,
}

impl Vm {
    pub(crate) fn prepare_class_callback(
        &mut self,
        class: Rc<Class>,
        receiver: Option<Rc<Object>>,
        name: &str,
    ) -> Result<(), NativeError> {
        let resolved = self.find_method(&class, name, Invocation::Method);
        if matches!(resolved, Ok(None)) && name.eq_ignore_ascii_case("__construct") {
            return Err(NativeError::error(format!(
                "Call to undefined method {}::{name}()",
                class.name
            )));
        }
        self.prepare_method_lookup(class, receiver, name, MethodSyntax::Static, resolved)
    }

    /// Normal resolution gets the first chance, including its lexical private lookup.
    pub(super) fn prepare_method_lookup(
        &mut self,
        class: Rc<Class>,
        receiver: Option<Rc<Object>>,
        name: &str,
        syntax: MethodSyntax,
        resolved: Result<Option<ResolvedMethod>, NativeError>,
    ) -> Result<(), NativeError> {
        let error = match resolved {
            Ok(Some(resolved)) => {
                return self.prepare_resolved_method(class, receiver, resolved, Invocation::Method);
            }
            Ok(None) => None,
            Err(error) => Some(error),
        };
        if syntax == MethodSyntax::Static && name.eq_ignore_ascii_case("__construct") {
            return Err(error.unwrap_or_else(|| NativeError::error("Cannot call constructor")));
        }
        let instance = receiver
            .as_ref()
            .filter(|_| {
                syntax == MethodSyntax::Instance
                    || class
                        .magic
                        .as_ref()
                        .is_some_and(|magic| magic.call.is_some())
            })
            .and_then(|object| self.classes.for_object(object))
            .and_then(|class| {
                let method = class.magic.as_ref()?.call.clone()?;
                Some((class, method))
            });
        let (called, method, receiver) = if let Some((class, method)) = instance {
            (class, method, receiver)
        } else if syntax == MethodSyntax::Static
            && let Some(method) = class
                .magic
                .as_ref()
                .and_then(|magic| magic.call_static.clone())
        {
            if class.kind == ClassKind::Trait {
                self.deprecate_static_trait_method(&class, name);
            }
            (class.clone(), method, None)
        } else {
            return Err(
                error.unwrap_or_else(|| crate::unsupported_items::undefined_method(&class, name))
            );
        };
        let scope = self
            .classes
            .get(&method.declaring)
            .ok_or_else(|| internal_error!("magic method scope"))?;
        let context = MethodContext {
            receiver,
            scope,
            called,
            invocation: Invocation::Method,
        };
        let written = name.split_once('\0').map_or(name, |(prefix, _)| prefix);
        self.prepare_call(Callee {
            argument_binding: ArgumentBinding::Caller,
            name: format!("{}::{name}", class.name).into(),
            parameters: [ParameterSpec {
                name: String::new(),
                ty: None,
                by_reference: false,
                variadic: true,
            }]
            .into(),
            target: CallTarget::Magic(Rc::new(MagicCall {
                written: written.into(),
                method,
                context,
            })),
        })
    }
}
