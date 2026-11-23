//! Binding changes the receiver and private scope: `Closure::bind($f, $object, Foo::class)`.

use std::rc::Rc;

use phera_bytecode::ClosureBinding;
use phera_extension_api::NativeError;
use phera_value::{Object, Value};

use crate::Vm;
use crate::calls::call_state::{CallTarget, Callee};
use crate::classes::{Class, Invocation, MethodContext};
use crate::closures::{ClosureOrigin, ClosureState};

/// Null selects global scope; an unknown class name cancels binding after its warning.
enum BindingScope {
    Present(Option<Rc<Class>>),
    Missing,
}

impl Vm {
    /// Scope lookup precedes compatibility checks, matching PHP's warning order.
    pub(crate) fn rebind_closure(
        &mut self,
        object: &Object,
        receiver: Option<Rc<Object>>,
        scope: &Value,
    ) -> Result<Option<ClosureState>, NativeError> {
        let native = object.native.borrow();
        let original = native
            .as_ref()
            .and_then(|state| state.downcast_ref::<ClosureState>())
            .ok_or_else(|| internal_error!("closure binding state"))?;
        let BindingScope::Present(scope) = self.binding_scope(scope, original)? else {
            return Ok(None);
        };
        if let Some(message) = self.invalid_closure_binding(original, receiver.as_deref(), &scope) {
            self.warn(format!("{message}, this will be an error in PHP 9"));
            return Ok(None);
        }
        let mut state = original.duplicate();
        let receiver_class = receiver
            .as_ref()
            .and_then(|object| self.classes.for_object(object));
        let effective_scope =
            scope.or_else(|| receiver.as_ref().and_then(|_| self.classes.get("Closure")));
        state.context = effective_scope.map(|scope| MethodContext {
            called: receiver_class.unwrap_or_else(|| scope.clone()),
            scope,
            receiver,
            invocation: Invocation::Method,
        });
        if let CallTarget::Method { method, context } = &state.callee.target {
            let mut context = context.clone();
            if let Some(bound) = &state.context {
                context = bound.clone();
            }
            state.callee = Callee {
                argument_binding: state.callee.argument_binding,
                name: state.callee.name.clone(),
                parameters: state.callee.parameters.clone(),
                target: CallTarget::Method {
                    method: method.clone(),
                    context,
                },
            };
        }
        Ok(Some(state))
    }

    /// An absent scope is valid; a missing named class must instead warn and cancel binding.
    fn binding_scope(
        &mut self,
        scope: &Value,
        original: &ClosureState,
    ) -> Result<BindingScope, NativeError> {
        let scope = match &*scope.deref() {
            Value::Null => None,
            Value::Object(object) => self.classes.for_object(object),
            Value::InternedStr(_) | Value::String(_) => {
                let bytes = scope.deref();
                let name = bytes.str_ref().expect("checked string");
                if name.as_bytes() == b"static" {
                    original
                        .context
                        .as_ref()
                        .map(|context| context.scope.clone())
                } else {
                    let name = name.to_string_lossy();
                    let Some(class) = self.classes.get(&name) else {
                        self.warn(format!("Class \"{name}\" not found"));
                        return Ok(BindingScope::Missing);
                    };
                    Some(class)
                }
            }
            other => {
                return Err(NativeError::type_error(format!(
                    "{method}: Argument #2 ($newScope) must be of type {types}, {} given",
                    other.type_name(),
                    method = "Closure::bindTo()",
                    types = "object|string|null"
                )));
            }
        };
        Ok(BindingScope::Present(scope))
    }

    /// Method wrappers keep their original declaring scope; lexical closures may change it.
    fn invalid_closure_binding(
        &self,
        state: &ClosureState,
        receiver: Option<&Object>,
        scope: &Option<Rc<Class>>,
    ) -> Option<String> {
        if let Some(message) = self.receiver_binding_error(state, receiver) {
            return Some(message);
        }
        let original_scope = state.context.as_ref().map(|context| &context.scope);
        let same_scope = match (original_scope, scope) {
            (Some(left), Some(right)) => Rc::ptr_eq(left, right),
            (None, None) => true,
            _ => false,
        };
        if !same_scope
            && let Some(scope) = scope
            && scope.origin != crate::classes::class::ClassOrigin::User
        {
            return Some(format!(
                "Cannot bind closure to scope of internal class {}",
                scope.name
            ));
        }
        if state.origin == ClosureOrigin::Callable && !same_scope {
            let source = if original_scope.is_some() {
                "method"
            } else {
                "function"
            };
            return Some(format!(
                "Cannot rebind scope of closure created from {source}"
            ));
        }
        None
    }
    /// Static closures and method wrappers constrain receiver changes before checking scope.
    fn receiver_binding_error(
        &self,
        state: &ClosureState,
        receiver: Option<&Object>,
    ) -> Option<String> {
        let original_scope = state.context.as_ref().map(|context| &context.scope);
        if let Some(receiver) = receiver {
            if state.binding == ClosureBinding::Static {
                return Some("Cannot bind an instance to a static closure".into());
            }
            if state.origin == ClosureOrigin::Callable
                && let Some(original) = original_scope
                && !self
                    .classes
                    .for_object(receiver)
                    .is_some_and(|class| class.is_a(&original.name))
            {
                return Some(format!(
                    "Cannot bind method {}() to object of class {}",
                    state.callee.name,
                    receiver.class.name()
                ));
            }
        } else if state.origin == ClosureOrigin::Callable
            && original_scope.is_some()
            && state.binding != ClosureBinding::Static
        {
            return Some("Cannot unbind $this of method".into());
        } else if state.origin == ClosureOrigin::Lexical
            && state
                .context
                .as_ref()
                .is_some_and(|context| context.receiver.is_some())
            && closure_uses_this(&self.code, state)
        {
            return Some("Cannot unbind $this of closure using $this".into());
        }
        None
    }
}

/// The compiled receiver operations also cover `$this` used through property paths.
fn closure_uses_this(code: &crate::code_table::CodeTable, state: &ClosureState) -> bool {
    let CallTarget::User(function) = state.callee.target else {
        return false;
    };
    let function = code.function(function);
    function.instructions.iter().any(|instruction| {
        matches!(
            instruction,
            phera_bytecode::Instruction::FetchThis { .. }
                | phera_bytecode::Instruction::ThisOrNull { .. }
        )
    }) || function
        .paths
        .iter()
        .any(|path| matches!(path.base, phera_bytecode::PathBase::This { .. }))
}
