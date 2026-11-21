//! Exact positional arguments let a native call stay in the synchronous batch.
//!
//! ```php
//! abs(-12);       // the supplied int already matches int|float
//! abs('-12');     // the argument parser applies the calling file's coercion rules
//! ```

use phera_bytecode::{Instruction, Reg, SymbolId, TypeMask};
use phera_extension_api::{
    FunctionEntry, NativeError, NativeFn, PropertyViewRequest, SyncFn, SyncSelector,
};
use phera_value::Value;

use super::call_state::CallTarget;
use super::callback_binding::ArgumentBinding;
use super::execution_loop::BatchExit;
use super::native_signature::{accepts, exact_types, native_type};
use super::pending::LinkedFunction;
use crate::Vm;

/// The registry signature reduced to the checks needed by a positional call.
#[derive(Clone, Copy)]
pub(crate) struct NativeLink {
    handler: Handler,
    name: &'static str,
    min: usize,
    max: usize,
    masks: [TypeMask; 4],
    object_handler: bool,
}

#[derive(Clone, Copy)]
enum Handler {
    Sync(SyncFn),
    Select(SyncSelector),
}

impl NativeLink {
    fn new(entry: FunctionEntry, registry: &phera_extension_api::Registry) -> Option<Self> {
        let handler = match entry.handler {
            NativeFn::Sync(handler) => Handler::Sync(handler),
            NativeFn::DeprecatedSync(_) | NativeFn::DeprecatedAsync(_) => return None,
            NativeFn::Async(_) => Handler::Select(registry.sync_selector(entry.name)?),
        };
        let mut masks = [TypeMask::default(); 4];
        // A selector can validate a positional window containing variadic
        // arguments. The signature still rejects references and coercions.
        let variadic = matches!(handler, Handler::Select(_))
            .then(|| entry.params.last().filter(|parameter| parameter.variadic))
            .flatten();
        for (index, mask) in masks.iter_mut().enumerate() {
            let Some(parameter) = entry.params.get(index).or(variadic) else {
                break;
            };
            let mut parameter = *parameter;
            if variadic.is_some() {
                parameter.variadic = false;
            }
            *mask = exact_types(&parameter);
        }
        Some(Self {
            handler,
            name: entry.name,
            min: entry.min_args(),
            max: if variadic.is_some() {
                masks.len()
            } else {
                entry.params.len().min(masks.len())
            },
            masks,
            object_handler: registry.object_handler(entry.name).is_some(),
        })
    }

    fn accepts(&self, values: &[Option<Value>]) -> bool {
        values.iter().zip(self.masks).all(|(slot, mask)| {
            slot.as_ref()
                .is_some_and(|value| mask.intersects(native_type(value)))
        }) && (!self.object_handler || !matches!(values.first(), Some(Some(Value::Object(_)))))
    }

    fn select_handler(&self, values: &[Option<Value>]) -> Option<SyncFn> {
        match self.handler {
            Handler::Sync(handler) => Some(handler),
            Handler::Select(selector) => selector(values),
        }
    }
}

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
        let link = self
            .code
            .cache(frame.function)
            .direct_natives
            .get()
            .and_then(|links| links.get(symbol.index()))
            .and_then(std::cell::Cell::get);
        let values = self.registers.window(base, count);
        if let Some(link) = link
            && count >= link.min
            && count <= link.max
            && values.len() == count
            && link.accepts(values)
            && let Some(handler) = link.select_handler(values)
        {
            if count == 1 {
                let argument = self.registers.take(base);
                return self.invoke_sync_function(
                    link.name,
                    handler,
                    std::slice::from_ref(&argument),
                    destination,
                    ArgumentBinding::Caller,
                );
            }
            let mut arguments = [const { Value::Null }; 4];
            for (offset, argument) in arguments[..count].iter_mut().enumerate() {
                *argument = self.registers.take(base + offset);
            }
            return self.invoke_sync_function(
                link.name,
                handler,
                &arguments[..count],
                destination,
                ArgumentBinding::Caller,
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
        if let CallTarget::Native(entry) = callee.target
            && let Some(frame) = self.call_stack.current()
        {
            let cache = self.code.cache(frame.function);
            let links = cache.direct_natives.get_or_init(|| {
                (0..cache.symbols.borrow().len())
                    .map(|_| std::cell::Cell::new(None))
                    .collect()
            });
            if let Some(slot) = links.get(symbol.index())
                && slot.get().is_none()
            {
                slot.set(NativeLink::new(entry, &self.registry));
            }
        }
        self.prepare_call(callee)?;
        for offset in 0..count {
            let argument = self.registers.take(base + offset);
            self.store_sent_value(offset, argument)?;
        }
        let result = self.call_native(destination);
        if let Err(error) = &result
            && crate::autoload::ClassMiss::is(error)
        {
            self.unprepare_direct_call();
        }
        result
    }

    /// After a class miss in `CallNativeDirect`, which the interpreter runs
    /// again once the class is loaded (see `calls::native_retries`): its
    /// arguments go back to the caller's registers it reads them from.
    /// Nothing for any other instruction.
    #[cold]
    #[inline(never)]
    fn return_direct_arguments(&mut self, arguments: &[Value]) {
        let Some(frame) = self.current() else {
            return;
        };
        let instruction = frame
            .function()
            .instructions
            .get(frame.ip().wrapping_sub(1));
        let Some(Instruction::CallNativeDirect { args, .. }) = instruction.copied() else {
            return;
        };
        let base = frame.base() + args.index();
        for (offset, argument) in arguments.iter().enumerate() {
            self.registers.put(base + offset, argument.clone());
        }
    }

    /// The same when the direct call went through a prepared call:
    /// `call_native` put that call back, but `CallNativeDirect` prepares its
    /// own when it runs again, so the prepared call goes (its arguments are
    /// back in the caller's registers already).
    #[cold]
    #[inline(never)]
    fn unprepare_direct_call(&mut self) {
        if let Ok((call, _)) = self.call_stack.pop_call() {
            self.registers.truncate(call.start());
        }
    }

    /// `spl_object_id($object)`: the object's id, with nothing else to do
    /// (a lazy object stays uninitialized). Any other shape goes to the
    /// general intrinsic.
    #[inline(never)]
    fn object_id_call(
        &mut self,
        call: crate::calls::prepared_call::PreparedCall,
        destination: Reg,
    ) -> Option<BatchExit> {
        let plain = call.is_plain()
            && call.sent == 1
            && self
                .call_stack
                .top_details()
                .is_some_and(|details| details.named.is_none() && details.extra_named.is_empty());
        let id = match self.registers.window(call.base(), 1) {
            [Some(Value::Object(object))] if plain => object.id,
            _ => return Some(BatchExit::NativeCall(destination)),
        };
        if self.call_stack.pop_call().is_err() {
            return Some(BatchExit::NativeCall(destination));
        }
        self.registers.truncate(call.start());
        self.set(destination, Value::Int(id.into()));
        None
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
            return match callee.target {
                CallTarget::Method { .. } => self.call_sync_native_method(call, destination),
                // Its callbacks run on the frame stack (`calls::native_loops`).
                CallTarget::Intrinsic(crate::intrinsics::Intrinsic::ArrayMap) => {
                    match self.start_array_map(destination)? {
                        Some(exit) => Ok(exit),
                        None => Ok(Some(BatchExit::NativeCall(destination))),
                    }
                }
                CallTarget::Intrinsic(crate::intrinsics::Intrinsic::Class(
                    crate::class_intrinsics::ClassFunction::ObjectId,
                )) => Ok(self.object_id_call(call, destination)),
                _ => Ok(Some(BatchExit::NativeCall(destination))),
            };
        };
        let NativeFn::Sync(handler) = entry.handler else {
            // Their callbacks run on the frame stack (`calls::native_loops`).
            if matches!(
                entry.name,
                "array_filter" | "array_reduce" | "usort" | "uasort" | "uksort"
            ) && let Some(exit) = self.start_native_function_loop(destination, entry)?
            {
                return Ok(exit);
            }
            return Ok(Some(BatchExit::NativeCall(destination)));
        };
        let binding = callee.argument_binding;
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
        let record = self.call_stack.pop_call()?;
        self.registers.truncate(call.start());

        let result =
            self.invoke_sync_function(entry.name, handler, &arguments, destination, binding);
        // The interpreter runs the `Call` again once the class is loaded
        // (see `calls::native_retries`).
        if let Err(error) = &result
            && crate::autoload::ClassMiss::is(error)
        {
            // A function's window keeps nothing below its first register.
            self.restore_native_call(record, [None, None], &arguments);
        }
        self.call_stack.recycle_native_arguments(arguments);
        result
    }

    pub(crate) fn accepts_sync_call(
        &self,
        entry: FunctionEntry,
        base: usize,
        count: usize,
    ) -> bool {
        let slots = self.registers.window(base, count);
        slots.len() == count
            && accepts(entry.params, slots)
            && (!matches!(slots.first(), Some(Some(Value::Object(_))))
                || self.registry.object_handler(entry.name).is_none())
    }

    pub(crate) fn invoke_sync_function(
        &mut self,
        name: &'static str,
        handler: SyncFn,
        arguments: &[Value],
        destination: Reg,
        binding: ArgumentBinding,
    ) -> Result<Option<BatchExit>, NativeError> {
        let previous_class = self.native_called_class.take();
        let previous_view = self.native_view_depth.replace(self.call_stack.depth());
        let pending = self.error_handlers.pending.len();
        let result = handler(self, arguments);
        self.native_view_depth = previous_view;
        self.native_called_class = previous_class;
        if binding.has_native_frame() && pending != self.error_handlers.pending.len() {
            self.attach_native_diagnostics(pending, name, arguments);
        }
        match result {
            Ok(value) => {
                if self.output_writer.buffers.pending.is_some() {
                    self.output_writer.chunk_caller = Some(name);
                }
                if let Some(frame) = self.call_stack.current()
                    && let [slot] = self
                        .registers
                        .window_mut(frame.base() + destination.index(), 1)
                {
                    *slot = Some(value);
                }
                Ok(None)
            }
            Err(error) => {
                self.finish_sync_function_error(name, arguments, destination, error, binding)
            }
        }
    }

    /// A synchronous handler cannot run PHP before its queued warnings are published.
    #[cold]
    pub(super) fn attach_native_diagnostics(
        &mut self,
        start: usize,
        name: &str,
        arguments: &[Value],
    ) {
        let trace = self.native_function_trace(name, arguments);
        if let Some(events) = self.error_handlers.pending.get_mut(start..) {
            for event in events {
                event.native = Some(trace.clone());
            }
        }
    }

    #[cold]
    fn finish_sync_function_error(
        &mut self,
        name: &str,
        arguments: &[Value],
        destination: Reg,
        error: NativeError,
        binding: ArgumentBinding,
    ) -> Result<Option<BatchExit>, NativeError> {
        if crate::autoload::ClassMiss::is(&error) {
            self.return_direct_arguments(arguments);
        }
        if binding.has_native_frame() {
            self.record_native_failure(
                &error,
                super::native_frames::NativeFrame::Function(name),
                arguments,
            );
        }
        if matches!(&error, NativeError::Unwind(unwind) if unwind.0.is::<PropertyViewRequest>()) {
            Ok(Some(BatchExit::NativeView(destination, Box::new(error))))
        } else {
            Err(error)
        }
    }
}
