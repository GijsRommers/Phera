//! Move just the generator's own register window and the state that points into it.
//!
//! ```php
//! function sample($items) { foreach ($items as $item) printf('%s', yield $item); }
//! ```
//!
//! Here the active foreach and the unfinished printf call both survive a yield.

use phera_extension_api::NativeError;
use phera_value::Value;

use crate::{
    Vm,
    calls::{
        frame::Frame,
        frame_extras::FrameExtras,
        prepared_call::{CallDetails, PreparedCall},
    },
    closures::PendingCapture,
    finally::SavedFinally,
};

pub(super) struct Activation {
    pub(super) frame: Frame,
    pub(super) silence: usize,
    storage: Storage,
}

#[derive(Default)]
pub(super) struct Storage {
    registers: Vec<Option<Value>>,
    extras: Option<Box<FrameExtras>>,
    calls: Vec<(PreparedCall, Option<CallDetails>)>,
    finally: Vec<SavedFinally>,
    captures: Vec<PendingCapture>,
}

impl Activation {
    /// The scalar interpreter has no access to the VM side tables.
    pub(super) fn local_registers(&mut self) -> Option<&mut [Option<Value>]> {
        if self.silence != 0
            || self.frame.has_extras()
            || !self.storage.calls.is_empty()
            || !self.storage.finally.is_empty()
            || !self.storage.captures.is_empty()
        {
            return None;
        }
        self.storage.registers.get_mut(self.frame.kind().prefix()..)
    }

    pub(super) fn has_receiver(&self) -> bool {
        let prefix = self.frame.kind().prefix();
        prefix > 0
            && matches!(
                self.storage.registers.get(prefix - 1),
                Some(Some(Value::Object(_)))
            )
    }

    pub(super) fn trace_arguments(&self, function: &phera_bytecode::Function) -> Value {
        let slots = self
            .storage
            .registers
            .get(self.frame.kind().prefix()..)
            .unwrap_or_default();
        let arguments = self
            .storage
            .extras
            .as_ref()
            .map(|extras| extras.arguments)
            .unwrap_or_default();
        let supplied = arguments
            .supplied
            .or(self.frame.passed())
            .unwrap_or(function.parameters.len());
        let count = self
            .storage
            .extras
            .as_ref()
            .and_then(|extras| extras.dynamic_count)
            .unwrap_or(function.registers as usize);
        let extra = slots
            .get(count..count + arguments.extra)
            .unwrap_or_default();
        crate::exception_traces::argument_values(function, supplied, slots, extra)
    }

    pub(super) fn set_result(
        &mut self,
        destination: phera_bytecode::Reg,
        value: Value,
    ) -> Result<(), NativeError> {
        let slot = self
            .storage
            .registers
            .get_mut(self.frame.kind().prefix() + destination.index())
            .ok_or_else(|| internal_error!("generator result register"))?;
        *slot = Some(value);
        Ok(())
    }
}

impl Vm {
    pub(super) fn suspend_generator_frame(
        &mut self,
        mut storage: Storage,
    ) -> Result<Activation, NativeError> {
        let (frame, extras) = self
            .call_stack
            .detach_frame()
            .ok_or_else(|| internal_error!("generator frame"))?;
        let start = frame.start();
        let base = frame.base();
        while self.call_stack.has_calls_from(start) {
            storage.calls.push(self.call_stack.pop_call()?);
        }
        if let Some(extras) = extras {
            if let Some(recycled) = &mut storage.extras {
                **recycled = extras;
            } else {
                storage.extras = Some(Box::new(extras));
            }
        }
        let finally_start = self
            .finally_stack
            .partition_point(|entry| entry.frame_base < base);
        let capture_start = self
            .closure_captures
            .partition_point(|entry| entry.base < base);
        storage
            .finally
            .extend(self.finally_stack.drain(finally_start..));
        storage
            .captures
            .extend(self.closure_captures.drain(capture_start..));
        self.registers.detach(start, &mut storage.registers)?;
        Ok(Activation {
            frame,
            silence: 0,
            storage,
        })
    }

    pub(super) fn restore_generator_frame(&mut self, activation: Activation) -> Storage {
        self.diagnostic_reporter
            .restore_silence(self.diagnostic_reporter.silence() + activation.silence);
        let old_start = activation.frame.start();
        let start = self.registers.len();
        let base = start + activation.frame.kind().prefix();
        let mut storage = activation.storage;
        self.registers.attach(&mut storage.registers);
        self.call_stack
            .push(activation.frame.moved_to(base).as_callback());
        if let Some(extras) = &mut storage.extras {
            let mut restored = std::mem::replace(extras.as_mut(), FrameExtras::new());
            if let Some(saved) = &mut restored.arguments.saved {
                *saved = start + (*saved - old_start);
            }
            *self.call_stack.current_extras_mut() = restored;
        }
        while let Some((call, details)) = storage.calls.pop() {
            let destination = start + (call.base() - old_start);
            self.call_stack
                .push_call(call.moved_to(destination), details);
        }
        for mut entry in storage.finally.drain(..) {
            entry.frame_base = base;
            self.finally_stack.push(entry);
        }
        for mut capture in storage.captures.drain(..) {
            capture.base = base;
            self.closure_captures.push(capture);
        }
        storage
    }
}
