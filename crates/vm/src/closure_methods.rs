//! Closure methods create or rebind callable objects: `$f->bindTo($object, $object)`.

use std::rc::Rc;

use phera_extension_api::{NativeError, NativeResult, Param, ParamType};
use phera_value::{Object, Value};

use crate::Vm;
use crate::closures::{ClosureOrigin, ClosureState};

/// VM-owned methods need access to lexical scope and the heap interpreter.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClosureMethod {
    Bind,
    BindTo,
    Call,
    FromCallable,
}

impl ClosureMethod {
    fn name(self) -> &'static str {
        match self {
            Self::Bind => "bind",
            Self::BindTo => "bindTo",
            Self::Call => "call",
            Self::FromCallable => "fromCallable",
        }
    }

    pub(crate) fn entries() -> &'static [(Self, &'static str)] {
        &[
            (Self::Bind, "bind"),
            (Self::BindTo, "bindTo"),
            (Self::Call, "call"),
            (Self::FromCallable, "fromCallable"),
        ]
    }

    pub(crate) fn parameters(self) -> &'static [Param] {
        const BIND: &[Param] = &[
            Param::new("closure", ParamType::Object),
            Param::new("newThis", ParamType::Object).nullable(),
            Param::new("newScope", ParamType::Mixed).optional(),
        ];
        const BIND_TO: &[Param] = &[
            Param::new("newThis", ParamType::Object).nullable(),
            Param::new("newScope", ParamType::Mixed).optional(),
        ];
        const CALL: &[Param] = &[
            Param::new("newThis", ParamType::Object),
            Param::new("args", ParamType::Mixed).variadic(),
        ];
        const FROM: &[Param] = &[Param::new("callback", ParamType::Mixed)];
        match self {
            Self::Bind => BIND,
            Self::BindTo => BIND_TO,
            Self::Call => CALL,
            Self::FromCallable => FROM,
        }
    }
}

impl Vm {
    /// Closure::call forwards the names in its variadic tail to the bound target.
    pub(crate) async fn closure_method_named(
        &mut self,
        operation: ClosureMethod,
        receiver: Option<Rc<Object>>,
        mut arguments: Vec<Value>,
        named_start: usize,
        named_keys: Vec<phera_value::PhpString>,
        origin: crate::calls::callback_binding::ArgumentOrigin,
    ) -> NativeResult {
        let name = operation.name();
        self.with_argument_origin(origin, |vm| {
            phera_extension_api::prepare_call(
                &format!("Closure::{name}"),
                operation.parameters(),
                &mut arguments,
                vm,
            )
        })?;
        if matches!(operation, ClosureMethod::FromCallable) {
            self.autoload_callable(&arguments[0]).await?;
            return self.closure_from_value(&arguments[0]);
        }
        let object = self.closure_receiver(operation, receiver, &mut arguments)?;
        let new_this = match &arguments[0] {
            Value::Object(object) => Some(object.clone()),
            _ => None,
        };
        let scope = if matches!(operation, ClosureMethod::Call) {
            arguments[0].clone()
        } else {
            arguments
                .get(1)
                .cloned()
                .unwrap_or_else(|| Value::from("static"))
        };
        let scope =
            self.with_argument_origin(origin, |vm| vm.closure_scope_argument(operation, scope))?;
        let Some(state) = self.rebind_closure(&object, new_this, &scope)? else {
            return Ok(Value::Null);
        };
        if matches!(operation, ClosureMethod::Call) {
            return self
                .call_bound_closure(object, state, arguments, named_start, named_keys)
                .await;
        }
        self.allocate_closure(state).map(Value::Object)
    }

    /// Temporary call binding shares static cells and forwards the original argument keys.
    async fn call_bound_closure(
        &mut self,
        object: Rc<Object>,
        mut state: ClosureState,
        arguments: Vec<Value>,
        named_start: usize,
        named_keys: Vec<phera_value::PhpString>,
    ) -> NativeResult {
        state.statics = {
            let native = object.native.borrow();
            let original = native
                .as_ref()
                .and_then(|state| state.downcast_ref::<ClosureState>())
                .ok_or_else(|| internal_error!("closure call state"))?;
            original.statics.clone()
        };
        let bound = self.allocate_closure(state)?;
        let callee = self.resolve_callable(&Value::Object(bound))?;
        let mut values = phera_value::PhpArray::new();
        for (index, value) in arguments.into_iter().enumerate().skip(1) {
            let name = index
                .checked_sub(named_start)
                .and_then(|index| named_keys.get(index));
            if let Some(name) = name {
                values.insert(phera_value::ArrayKey::String(name.clone()), value);
            } else {
                values.push(value)?;
            }
        }
        self.invoke_array_callback(callee, &values).await
    }

    /// Weak native union parameters accept scalar strings, while strict calls reject them.
    fn closure_scope_argument(&mut self, operation: ClosureMethod, scope: Value) -> NativeResult {
        if matches!(scope, Value::Null | Value::Object(_) | Value::String(_)) {
            return Ok(scope);
        }
        if !phera_extension_api::Context::strict_types(self)
            && matches!(scope, Value::Bool(_) | Value::Int(_) | Value::Float(_))
        {
            let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
            let scope = phera_value::to_php_string(&scope, &mut diagnostics.buffer)?;
            return Ok(Value::String(scope));
        }
        let name = operation.name();
        let number = if operation == ClosureMethod::Bind {
            3
        } else {
            2
        };
        Err(NativeError::type_error(format!(
            "Closure::{name}(): Argument #{number} ($newScope) must be of type {types}, {} given",
            scope.type_name(),
            types = "object|string|null"
        )))
    }

    /// Static bind receives its closure as an argument; other operations already have a receiver.
    fn closure_receiver(
        &self,
        operation: ClosureMethod,
        receiver: Option<Rc<Object>>,
        arguments: &mut Vec<Value>,
    ) -> Result<Rc<Object>, NativeError> {
        let object = if matches!(operation, ClosureMethod::Bind) {
            let Value::Object(object) = arguments.remove(0) else {
                return Err(internal_error!("closure bind argument"));
            };
            if object.class.name() != "Closure" {
                return Err(NativeError::type_error(format!(
                    "Closure::bind(): Argument #1 ($closure) must be of type Closure, {} given",
                    object.class.name()
                )));
            }
            object
        } else {
            receiver.ok_or_else(|| internal_error!("closure method receiver"))?
        };
        Ok(object)
    }

    /// FromCallable retains validated visibility even after the creating scope has returned.
    pub(crate) fn closure_from_value(&mut self, value: &Value) -> NativeResult {
        if let Some(object) = existing_closure(value) {
            return Ok(Value::Object(object));
        }
        let callee = self.resolve_callable(value).map_err(|error| {
            let reason = self.callback_reason(value, error);
            NativeError::type_error(format!("Failed to create closure from callable: {reason}"))
        })?;
        self.closure_from_callee(callee).map(Value::Object)
    }

    /// First-class callables retain validated access after their creating scope returns.
    pub(crate) fn closure_from_callee(
        &mut self,
        callee: crate::calls::call_state::Callee,
    ) -> Result<Rc<Object>, NativeError> {
        if let crate::calls::call_state::CallTarget::Closure(object) = &callee.target {
            return Ok(object.clone());
        }
        let (context, binding) = match &callee.target {
            crate::calls::call_state::CallTarget::Method { context, method } => (
                Some(context.clone()),
                if method.is_static {
                    phera_bytecode::ClosureBinding::Static
                } else {
                    phera_bytecode::ClosureBinding::Bound
                },
            ),
            _ => (None, phera_bytecode::ClosureBinding::Bound),
        };
        self.allocate_closure(ClosureState {
            callee,
            context,
            binding,
            origin: ClosureOrigin::Callable,
            captures: Vec::new(),
            statics: Rc::new(std::cell::RefCell::new(Default::default())),
        })
    }
}

/// PHP treats a Closure's synthetic __invoke method as the original closure object.
fn existing_closure(value: &Value) -> Option<Rc<Object>> {
    let value = value.deref();
    let object = match &*value {
        Value::Object(object) => object.clone(),
        Value::Array(array) if array.len() == 2 => {
            let Value::String(method) = &*array.get(&phera_value::ArrayKey::Int(1))?.deref() else {
                return None;
            };
            if !method.to_string_lossy().eq_ignore_ascii_case("__invoke") {
                return None;
            }
            let Value::Object(object) = &*array.get(&phera_value::ArrayKey::Int(0))?.deref() else {
                return None;
            };
            object.clone()
        }
        _ => return None,
    };
    (object.class.name() == "Closure").then_some(object)
}
