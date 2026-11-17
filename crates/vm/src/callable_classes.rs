//! Legacy callback scopes still work with deprecation: `Closure::fromCallable('self::method')`.

use std::rc::Rc;

use phera_extension_api::NativeError;

use crate::Vm;
use crate::call_state::CallTarget;
use crate::callables::CallableUse;
use crate::classes::{Class, Invocation};

impl Vm {
    /// Literal class strings never borrow $this; only the deprecated relative keywords may do so.
    pub(crate) fn prepare_callable_class(
        &mut self,
        name: &str,
        method: &str,
        usage: CallableUse,
    ) -> Result<(), NativeError> {
        if usage != CallableUse::Callback || !matches!(name, "self" | "parent" | "static") {
            let class = self
                .classes
                .get(name)
                .ok_or_else(|| NativeError::error(format!("Class \"{name}\" not found")))?;
            return self.prepare_callback_method(class, None, method, usage);
        }
        self.diagnostic(
            "Deprecated",
            format!("Use of \"{name}\" in callables is deprecated"),
        );
        let context = self
            .current()
            .and_then(|frame| frame.method.as_deref())
            .cloned()
            .ok_or_else(|| {
                NativeError::error(format!(
                    "Cannot access \"{name}\" when no class scope is active"
                ))
            })?;
        let class = match name {
            "parent" => context.scope.parent.clone(),
            "static" => Some(context.called.clone()),
            _ => Some(context.scope.clone()),
        }
        .ok_or_else(|| {
            NativeError::error("Cannot access \"parent\" when current class scope has no parent")
        })?;
        self.prepare_method(class, context.receiver, method, Invocation::Method)?;
        self.forward_callback_class(context.called)
    }

    /// Parent-qualified callback lookup changes visibility scope but retains late static binding.
    pub(crate) fn forward_callback_class(&mut self, class: Rc<Class>) -> Result<(), NativeError> {
        let pending = self
            .call_stack
            .current_mut()
            .and_then(|frame| frame.calls.last_mut())
            .ok_or_else(|| internal_error!("callback forwarding"))?;
        if let CallTarget::Method { context, .. } = &mut Rc::make_mut(&mut pending.callee).target {
            context.called = class;
        }
        Ok(())
    }
}
