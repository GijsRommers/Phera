//! Exact positional arguments let a native call stay in the synchronous batch.
//!
//! ```php
//! abs(-12);       // the supplied int already matches int|float
//! abs('-12');     // the argument parser applies the calling file's coercion rules
//! ```

use phera_bytecode::{Reg, TypeMask};
use phera_extension_api::{NativeError, NativeFn, Param, ParamType, PropertyViewRequest};
use phera_value::Value;

use super::call_state::CallTarget;
use super::execution_loop::BatchExit;
use crate::{Vm, error_traces::NativeTrace};

impl Vm {
    /// Unhandled shapes keep their window intact for the ordinary native binder.
    #[inline(never)]
    pub(crate) fn call_native(
        &mut self,
        destination: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let call = *self.call_stack.top_call()?;
        let Some(details) = self.call_stack.top_details() else {
            return Ok(Some(BatchExit::NativeCall(destination)));
        };
        let Some(callee) = &details.callee else {
            return Ok(Some(BatchExit::NativeCall(destination)));
        };
        let CallTarget::Native(entry) = callee.target else {
            return Ok(Some(BatchExit::NativeCall(destination)));
        };
        let NativeFn::Sync(handler) = entry.handler else {
            return Ok(Some(BatchExit::NativeCall(destination)));
        };
        let count = call.sent as usize;
        let slots = self.registers.window(call.base(), count);
        if !call.is_plain()
            || details.named.is_some()
            || (self.error_handlers.error.is_some() && !self.error_handlers.active)
            || slots.len() != count
            || !accepts(entry.params, slots)
            || (matches!(slots.first(), Some(Some(Value::Object(_))))
                && self.registry.object_handler(entry.name).is_some())
        {
            return Ok(Some(BatchExit::NativeCall(destination)));
        }

        // Option<Value> also represents an undefined register. Move the values
        // to the reusable native buffer so the handler can borrow the whole VM.
        let mut arguments = self.call_stack.native_arguments();
        arguments.reserve(count);
        for slot in self.registers.window_mut(call.base(), count) {
            if let Some(value) = slot.take() {
                arguments.push(value);
            }
        }
        self.call_stack.pop_call()?;
        self.registers.truncate(call.start());

        let previous_class = self.native_called_class.take();
        let previous_view = self.native_view_depth.replace(self.call_stack.depth());
        let result = handler(self, &arguments);
        self.native_view_depth = previous_view;
        self.native_called_class = previous_class;
        if result.is_err() && !super::native::omits_native_trace(entry.name) {
            self.failed_native = Some(NativeTrace::new(entry.name, &arguments));
        }
        self.call_stack.recycle_native_arguments(arguments);

        match result {
            Ok(value) => {
                if self.output_writer.buffers.pending.is_some() {
                    self.output_writer.chunk_caller = Some(entry.name);
                }
                self.set(destination, value);
                Ok(None)
            }
            Err(error @ NativeError::Unwind(_)) if matches!(&error, NativeError::Unwind(unwind) if unwind.0.is::<PropertyViewRequest>()) => {
                Ok(Some(BatchExit::NativeView(destination, Box::new(error))))
            }
            Err(error) => Err(error),
        }
    }
}

/// These argument types are accepted as supplied. A zero mask reserves class,
/// callable and by-reference checks for the normal binder.
fn accepts(parameters: &[Param], slots: &[Option<Value>]) -> bool {
    slots.len() <= parameters.len()
        && parameters[slots.len()..]
            .iter()
            .all(|parameter| parameter.optional)
        && parameters.iter().zip(slots).all(|(parameter, slot)| {
            slot.as_ref().is_some_and(|value| {
                !value.is_reference()
                    && !parameter.by_ref
                    && !parameter.variadic
                    && (parameter.ty == ParamType::Mixed
                        || exact_types(parameter).intersects(crate::types::own_type(value)))
            })
        })
}

fn exact_types(parameter: &Param) -> TypeMask {
    use ParamType as P;
    use TypeMask as T;
    let mask = match parameter.ty {
        P::Bool => T::BOOL,
        P::Int | P::RoundingModeOrInt => T::INT,
        P::Float => T::FLOAT,
        P::String | P::ClosureOrString => T::STRING,
        P::Array | P::Iterable => T::ARRAY,
        P::Object => T::OBJECT,
        P::ArrayOrObject | P::ObjectOrArray => T::ARRAY.union(T::OBJECT),
        P::Number => T::INT.union(T::FLOAT),
        P::IntOrString => T::INT.union(T::STRING),
        P::ArrayOrInt => T::ARRAY.union(T::INT),
        P::ArrayOrString => T::ARRAY.union(T::STRING),
        P::ObjectOrString => T::OBJECT.union(T::STRING),
        P::StringOrNumber => T::STRING.union(T::INT).union(T::FLOAT),
        P::ArrayOrNumber => T::ARRAY.union(T::INT).union(T::FLOAT),
        P::ArrayOrFloat => T::ARRAY.union(T::FLOAT),
        P::ArrayOrBool => T::ARRAY.union(T::BOOL),
        P::Mixed | P::Class(_) | P::Callable | P::RecursiveOrAggregate => T::default(),
    };
    if parameter.nullable {
        mask.union(T::NULL)
    } else {
        mask
    }
}
