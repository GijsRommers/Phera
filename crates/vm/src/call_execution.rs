//! Prepared calls enter heap frames: `$f($first, named: $second)` never recurses in Rust.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::{FunctionEntry, NativeError};
use phera_value::{Object, Value};

use crate::Vm;
use crate::call_state::{CallTarget, NamedArguments};
use crate::classes::{Method, MethodContext};
use crate::intrinsics::Intrinsic;

/// Named variadic values join the positional buffer only after all sends have completed.
struct CallArguments {
    values: Vec<Option<Value>>,
    named_start: usize,
    named_keys: Vec<String>,
}

impl CallArguments {
    fn prepare(
        mut values: Vec<Option<Value>>,
        extra: Option<Box<NamedArguments>>,
        parameters: usize,
    ) -> Self {
        let mut named_keys = Vec::new();
        let mut named_start = 0;
        if let Some(extra) = extra {
            named_start = values.len().max(parameters.saturating_sub(1));
            values.resize_with(named_start, || None);
            for argument in extra.values {
                named_keys.push(argument.name);
                values.push(Some(argument.value));
            }
        }
        Self {
            values,
            named_start,
            named_keys,
        }
    }
}

/// Native functions and scope-aware intrinsics share result storage and handler transitions.
enum NativeTarget {
    Function(FunctionEntry),
    Intrinsic(Intrinsic),
}

impl Vm {
    /// Snapshot targets survive argument evaluation and keep closure objects alive until return.
    pub(crate) async fn finish_call(&mut self, destination: Reg) -> Result<(), NativeError> {
        let call = self
            .call_stack
            .current_mut()
            .and_then(|frame| frame.calls.pop())
            .ok_or_else(|| internal_error!("call not initialized"))?;
        let arguments = CallArguments::prepare(
            call.arguments,
            call.extra_named,
            call.callee.parameters.len(),
        );
        let origin = call
            .callee
            .argument_binding
            .origin(crate::callback_binding::ArgumentOrigin::Php);
        match &call.callee.target {
            CallTarget::Vacant => return Err(internal_error!("vacant call")),
            CallTarget::User(function) => {
                self.start_user_call(function, destination, arguments, call.owner, origin)?;
                return Ok(());
            }
            CallTarget::Closure(object) => {
                self.execute_closure_target(&call.callee, object, destination, arguments)?;
            }
            CallTarget::Method { method, context } => {
                self.execute_bound_method(
                    method,
                    context,
                    destination,
                    arguments,
                    call.owner,
                    origin,
                )
                .await?;
            }
            CallTarget::Constructed(object) => self.set(destination, Value::Object(object.clone())),
            CallTarget::Native(entry) => {
                self.execute_native_target(
                    NativeTarget::Function(*entry),
                    destination,
                    arguments.values,
                    origin,
                )
                .await?;
            }
            CallTarget::Intrinsic(intrinsic) => {
                self.execute_native_target(
                    NativeTarget::Intrinsic(*intrinsic),
                    destination,
                    arguments.values,
                    origin,
                )
                .await?;
            }
        }
        self.call_stack.recycle_bound_callee(call.callee);
        Ok(())
    }

    /// Failed parameter binding still publishes named variadic keys for PHP's error trace.
    #[inline]
    fn start_user_call(
        &mut self,
        function: &crate::function_table::FunctionRef,
        destination: Reg,
        arguments: CallArguments,
        owner: Option<Rc<Object>>,
        origin: crate::callback_binding::ArgumentOrigin,
    ) -> Result<(), NativeError> {
        let result = self.with_argument_origin(origin, |vm| {
            vm.push_prepared(
                &function.unit,
                function.index,
                Some(destination),
                arguments.values,
            )
        });
        let entered = self.current().is_some_and(|frame| {
            frame.ip == 0
                && frame.function_id == Some(function.index)
                && std::sync::Arc::ptr_eq(&frame.unit, &function.unit)
        });
        if entered {
            if let Some(frame) = self.call_stack.current_mut() {
                frame.closure = owner;
            }
            self.name_variadic_arguments(arguments.named_start, arguments.named_keys)?;
        }
        result
    }

    /// Calling a closure pushes a frame for its body, like any other PHP function.
    fn execute_closure_target(
        &mut self,
        callee: &crate::call_state::Callee,
        object: &Rc<Object>,
        destination: Reg,
        arguments: CallArguments,
    ) -> Result<(), NativeError> {
        let origin = callee
            .argument_binding
            .origin(crate::callback_binding::ArgumentOrigin::Php);
        let result = self.with_argument_origin(origin, |vm| {
            vm.push_closure(object.clone(), Some(destination), arguments.values)
        });
        let entered = self.current().is_some_and(|frame| {
            frame.ip == 0
                && frame
                    .closure
                    .as_ref()
                    .is_some_and(|owner| Rc::ptr_eq(owner, object))
        });
        if entered {
            self.name_variadic_arguments(arguments.named_start, arguments.named_keys)?;
        }
        result
    }

    /// Leaf plans complete synchronously; other methods retain the original named-argument routing.
    async fn execute_bound_method(
        &mut self,
        method: &Method,
        context: &MethodContext,
        destination: Reg,
        mut arguments: CallArguments,
        owner: Option<Rc<Object>>,
        origin: crate::callback_binding::ArgumentOrigin,
    ) -> Result<(), NativeError> {
        if let crate::classes::MethodBody::Closure(operation) = &method.body {
            let values = arguments
                .values
                .into_iter()
                .map(Option::unwrap_or_default)
                .collect();
            let value = Box::pin(self.closure_method_named(
                *operation,
                context.receiver.clone(),
                values,
                arguments.named_start,
                arguments.named_keys,
                origin,
            ))
            .await?;
            self.set(destination, value);
        } else if let Some(value) = self.leaf_method_result(context) {
            arguments.values.clear();
            self.call_stack.recycle_arguments(arguments.values);
            self.set(destination, value);
        } else {
            self.invoke_method(
                method,
                context.clone(),
                destination,
                arguments.values,
                origin,
            )
            .await?;
            if matches!(method.body, crate::classes::MethodBody::User(_))
                && let Some(frame) = self.call_stack.current_mut()
            {
                frame.closure = owner;
            }
            self.name_variadic_arguments(arguments.named_start, arguments.named_keys)?;
        }
        Ok(())
    }

    /// Changing the error handler selects the existing handler-aware interpreter at its boundary.
    async fn execute_native_target(
        &mut self,
        target: NativeTarget,
        destination: Reg,
        arguments: Vec<Option<Value>>,
        origin: crate::callback_binding::ArgumentOrigin,
    ) -> Result<(), NativeError> {
        let arguments = match target {
            NativeTarget::Intrinsic(intrinsic) => Self::intrinsic_arguments(intrinsic, arguments),
            NativeTarget::Function(_) => arguments
                .into_iter()
                .map(Option::unwrap_or_default)
                .collect(),
        };
        let value = match target {
            NativeTarget::Function(entry) => {
                self.native_entry_origin(entry, arguments, origin).await?
            }
            NativeTarget::Intrinsic(intrinsic) => {
                self.intrinsic_origin(intrinsic, arguments, origin).await?
            }
        };
        self.set(destination, value);
        Ok(())
    }
}
