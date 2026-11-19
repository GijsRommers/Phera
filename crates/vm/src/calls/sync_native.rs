//! Exact positional arguments let a native call stay in the synchronous batch.
//!
//! ```php
//! abs(-12);       // the supplied int already matches int|float
//! abs('-12');     // the argument parser applies the calling file's coercion rules
//! ```

use phera_bytecode::{Reg, SymbolId, TypeMask};
use phera_extension_api::{
    FunctionEntry, NativeError, NativeFn, Param, ParamType, PropertyViewRequest, SyncFn,
};
use phera_value::Value;

use super::call_state::CallTarget;
use super::execution_loop::BatchExit;
use super::pending::LinkedFunction;
use crate::{Vm, error_traces::NativeTrace};

impl Vm {
    /// The compiler put positional arguments in a contiguous part of the caller frame.
    #[inline(never)]
    pub(crate) fn call_native_direct(
        &mut self,
        symbol: SymbolId,
        args: Reg,
        count: u16,
        destination: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let frame = *self
            .call_stack
            .current()
            .ok_or_else(|| internal_error!("native call frame"))?;
        let base = frame.base() + args.index();
        let count = usize::from(count);
        let entry = {
            let symbols = self.code.cache(frame.function).symbols.borrow();
            match symbols.get(symbol.index()).and_then(Option::as_ref) {
                Some(LinkedFunction::Other(callee)) => match callee.target {
                    CallTarget::Native(entry) => Some(entry),
                    _ => None,
                },
                _ => None,
            }
        };
        if let Some(entry) = entry
            && let NativeFn::Sync(handler) = entry.handler
            && count <= 4
            && self.accepts_sync_call(entry, base, count)
        {
            let mut arguments = [const { Value::Null }; 4];
            for (offset, argument) in arguments[..count].iter_mut().enumerate() {
                *argument = self.registers.take(base + offset);
            }
            return self.invoke_sync_function(
                entry.name,
                handler,
                &arguments[..count],
                destination,
            );
        }
        self.call_native_direct_fallback(symbol, base, count, destination)
    }

    /// Link once, then preserve binding and coercion for arguments that need them.
    #[cold]
    fn call_native_direct_fallback(
        &mut self,
        symbol: SymbolId,
        base: usize,
        count: usize,
        destination: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let LinkedFunction::Other(callee) = self.linked_function(symbol)? else {
            return Err(internal_error!("direct native target"));
        };
        self.prepare_call(callee)?;
        for offset in 0..count {
            let argument = self.registers.take(base + offset);
            self.store_sent_value(offset, argument)?;
        }
        self.call_native(destination)
    }

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
        if !call.is_plain()
            || details.named.is_some()
            || !self.accepts_sync_call(entry, call.base(), count)
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

        let result = self.invoke_sync_function(entry.name, handler, &arguments, destination);
        self.call_stack.recycle_native_arguments(arguments);
        result
    }

    fn accepts_sync_call(&self, entry: FunctionEntry, base: usize, count: usize) -> bool {
        let slots = self.registers.window(base, count);
        (self.error_handlers.error.is_none() || self.error_handlers.active)
            && slots.len() == count
            && accepts(entry.params, slots)
            && (!matches!(slots.first(), Some(Some(Value::Object(_))))
                || self.registry.object_handler(entry.name).is_none())
    }

    fn invoke_sync_function(
        &mut self,
        name: &'static str,
        handler: SyncFn,
        arguments: &[Value],
        destination: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let previous_class = self.native_called_class.take();
        let previous_view = self.native_view_depth.replace(self.call_stack.depth());
        let result = handler(self, arguments);
        self.native_view_depth = previous_view;
        self.native_called_class = previous_class;
        match result {
            Ok(value) => {
                if self.output_writer.buffers.pending.is_some() {
                    self.output_writer.chunk_caller = Some(name);
                }
                self.set(destination, value);
                Ok(None)
            }
            Err(error) => self.finish_sync_function_error(name, arguments, destination, error),
        }
    }

    #[cold]
    fn finish_sync_function_error(
        &mut self,
        name: &str,
        arguments: &[Value],
        destination: Reg,
        error: NativeError,
    ) -> Result<Option<BatchExit>, NativeError> {
        if !super::native::omits_native_trace(name) {
            self.failed_native = Some(NativeTrace::new(name, arguments));
        }
        if matches!(&error, NativeError::Unwind(unwind) if unwind.0.is::<PropertyViewRequest>()) {
            Ok(Some(BatchExit::NativeView(destination, Box::new(error))))
        } else {
            Err(error)
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
