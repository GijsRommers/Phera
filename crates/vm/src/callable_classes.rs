//! Legacy callback scopes still work with deprecation: `Closure::fromCallable('self::method')`.

use std::rc::Rc;

use phera_extension_api::NativeError;

use crate::Vm;
use crate::callables::CallableUse;
use crate::calls::call_state::CallTarget;
use crate::calls::prepared_call::CallFlags;
use crate::classes::Class;

impl Vm {
    /// Literal class strings never borrow $this; only the deprecated relative keywords may do so.
    pub(crate) fn prepare_callable_class(
        &mut self,
        name: &str,
        method: &str,
        usage: CallableUse<'_>,
    ) -> Result<(), NativeError> {
        if !usage.allows_callback_syntax() || !matches!(name, "self" | "parent" | "static") {
            let class = self.classes.get(name).ok_or_else(|| match usage {
                CallableUse::Callback | CallableUse::Type(_) => {
                    crate::unsupported_items::omitted_member(
                        phera_source::deprecations::ItemKind::Method,
                        name,
                        method,
                    )
                    .unwrap_or_else(|| NativeError::error(format!("Class \"{name}\" not found")))
                }
                _ => Self::class_member_miss(
                    name,
                    phera_source::deprecations::ItemKind::Method,
                    method,
                ),
            })?;
            let receiver = match usage {
                CallableUse::Callback => self.current().and_then(|frame| frame.this()),
                CallableUse::Type(context) => context.and_then(|context| context.receiver.as_ref()),
                _ => None,
            }
            .filter(|object| {
                self.classes
                    .for_object(object)
                    .is_some_and(|called| called.is_a(&class.name))
            })
            .cloned();
            if usage.allows_callback_syntax() && method.contains("::") {
                return self.prepare_callback_method(class, receiver, method, usage);
            }
            return self.prepare_static_callback(class, receiver, method, usage);
        }
        let context = match usage {
            CallableUse::Type(context) => context.cloned(),
            _ => self.current().and_then(|frame| frame.method()),
        }
        .ok_or_else(|| {
            NativeError::error(format!(
                "Cannot access \"{name}\" when no class scope is active"
            ))
        })?;
        self.diagnostic(
            "Deprecated",
            format!("Use of \"{name}\" in callables is deprecated"),
        );
        let class = match name {
            "parent" => context.scope.parent.clone(),
            "static" => Some(context.called.clone()),
            _ => Some(context.scope.clone()),
        }
        .ok_or_else(|| {
            NativeError::error("Cannot access \"parent\" when current class scope has no parent")
        })?;
        self.prepare_static_callback(class, context.receiver, method, usage)?;
        self.forward_callback_class(context.called)
    }

    fn prepare_static_callback(
        &mut self,
        class: Rc<Class>,
        receiver: Option<Rc<phera_value::Object>>,
        method: &str,
        usage: CallableUse<'_>,
    ) -> Result<(), NativeError> {
        if let CallableUse::Type(context) = usage {
            return self.prepare_typed_callback(
                class,
                receiver,
                method,
                crate::classes::magic_calls::MethodSyntax::Static,
                context.map(|context| &context.scope),
            );
        }
        self.prepare_class_callback(class, receiver, method)
    }

    /// `call_user_func('parent::m')` looks the method up in the parent but
    /// keeps the caller's called class, as `parent::m()` does. A call to PHP
    /// code keeps its called class in the record itself; a call with details
    /// keeps it in the method context there.
    pub(crate) fn forward_callback_class(&mut self, class: Rc<Class>) -> Result<(), NativeError> {
        let call = self.call_stack.top_call_mut()?;
        call.called = Some(class.index);
        if !call.flags.contains(CallFlags::DETAILS) {
            return Ok(());
        }
        let details = self.call_stack.top_details_mut()?;
        match details.callee.as_mut().map(|callee| &mut callee.target) {
            Some(CallTarget::Method { context, .. }) => context.called = class,
            Some(CallTarget::Magic(magic)) => Rc::make_mut(magic).context.called = class,
            _ => {}
        }
        Ok(())
    }
}
