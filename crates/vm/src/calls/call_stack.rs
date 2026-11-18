//! Request-local call stack state kept separate from register execution.

use phera_extension_api::NativeError;

use crate::calls::frame::Frame;
use crate::calls::frame_extras::FrameExtras;
use crate::calls::prepared_call::{CallDetails, CallFlags, PreparedCall};

/// The running PHP functions of a request, innermost last, and the calls
/// whose arguments are being evaluated.
pub(crate) struct CallStack {
    frames: Vec<Frame>,
    /// Optional per-frame state, indexed by frame depth.
    extras: Vec<FrameExtras>,
    /// What `extras` returns for a frame that stores nothing there.
    empty: FrameExtras,
    /// The calls whose arguments are being evaluated, innermost last.
    preparing: Vec<PreparedCall>,
    /// The details of the prepared calls that have some, in the same order.
    details: Vec<CallDetails>,
    /// Native callbacks have an independent bound because they await Rust futures.
    callback_depth: usize,
    /// Sync natives return their temporary API buffers to this request-local pool.
    native_arguments: Vec<Vec<phera_value::Value>>,
}

impl CallStack {
    /// Start an isolated request with empty reusable stacks.
    pub(crate) fn new() -> Self {
        Self {
            frames: vec![],
            extras: vec![],
            empty: FrameExtras::new(),
            callback_depth: 0,
            native_arguments: Vec::new(),
            preparing: Vec::new(),
            details: Vec::new(),
        }
    }
    /// The last activation owns the currently executing instruction.
    #[inline]
    pub(crate) fn current(&self) -> Option<&Frame> {
        self.frames.last()
    }

    /// Only the active activation may change its instruction pointer.
    #[inline]
    pub(crate) fn current_mut(&mut self) -> Option<&mut Frame> {
        self.frames.last_mut()
    }

    /// The frame that called the running one.
    #[inline]
    pub(crate) fn caller(&self) -> Option<&Frame> {
        self.frames
            .len()
            .checked_sub(2)
            .map(|depth| &self.frames[depth])
    }

    /// Whether a call is being prepared with its window at or above `base`.
    #[inline]
    pub(crate) fn has_calls_from(&self, base: usize) -> bool {
        self.preparing
            .last()
            .is_some_and(|call| call.start() >= base)
    }

    /// The frame at `depth` (0 is the script's top-level code).
    pub(crate) fn at(&self, depth: usize) -> Option<&Frame> {
        self.frames.get(depth)
    }

    /// Callback boundaries and the PHP frame limit use heap depth.
    #[inline]
    pub(crate) fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Enter a function. Its side-table entry is empty, because the frame
    /// that last used this depth emptied it when it left.
    #[inline]
    pub(crate) fn push(&mut self, frame: Frame) {
        self.frames.push(frame);
    }

    /// Leave a function, releasing what its side-table entry held.
    #[inline]
    pub(crate) fn pop(&mut self) -> Option<Frame> {
        let frame = self.frames.pop()?;
        if frame.has_extras() {
            self.extras[self.frames.len()].clear();
        }
        Some(frame)
    }

    /// Exit discards preparations and all active activations.
    pub(crate) fn clear(&mut self) {
        self.truncate_calls(0);
        while self.pop().is_some() {}
    }

    /// Drop the prepared calls whose windows start at or above `base`: a
    /// frame left while its arguments were being evaluated.
    pub(crate) fn abandon_calls_from(&mut self, base: usize) {
        while self.has_calls_from(base) {
            let _ = self.pop_call();
        }
    }

    /// The frames from the outermost to the innermost.
    pub(crate) fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// The side-table entry of the frame at `depth`.
    pub(crate) fn extras(&self, depth: usize) -> &FrameExtras {
        match self.frames.get(depth) {
            Some(frame) if frame.has_extras() => &self.extras[depth],
            _ => &self.empty,
        }
    }

    /// The side-table entry of the frame at `depth`, for writing. The frame
    /// is marked so that the entry is emptied when it leaves.
    pub(crate) fn extras_mut(&mut self, depth: usize) -> &mut FrameExtras {
        while self.extras.len() <= depth {
            self.extras.push(FrameExtras::new());
        }
        if let Some(frame) = self.frames.get_mut(depth) {
            frame.mark_extras();
        }
        &mut self.extras[depth]
    }

    /// The innermost frame's side-table entry, for writing.
    pub(crate) fn current_extras_mut(&mut self) -> &mut FrameExtras {
        let depth = self.frames.len().saturating_sub(1);
        self.extras_mut(depth)
    }

    /// How many calls are being prepared.
    pub(crate) fn call_depth(&self) -> usize {
        self.preparing.len()
    }

    /// Start preparing a call, with its details if it has any.
    #[inline]
    pub(crate) fn push_call(&mut self, mut call: PreparedCall, details: Option<CallDetails>) {
        if let Some(details) = details {
            call.flags.insert(CallFlags::DETAILS);
            self.details.push(details);
        }
        self.preparing.push(call);
    }

    /// The innermost prepared call, if there is one.
    #[inline]
    pub(crate) fn last_call(&self) -> Option<&PreparedCall> {
        self.preparing.last()
    }

    /// The innermost prepared call, for writing.
    #[inline]
    pub(crate) fn last_call_mut(&mut self) -> Option<&mut PreparedCall> {
        self.preparing.last_mut()
    }

    /// The innermost prepared call; every `Send` and `Call` has one.
    pub(crate) fn top_call(&self) -> Result<&PreparedCall, NativeError> {
        self.preparing
            .last()
            .ok_or_else(|| internal_error!("call not initialized"))
    }

    /// The innermost prepared call, for writing.
    pub(crate) fn top_call_mut(&mut self) -> Result<&mut PreparedCall, NativeError> {
        self.preparing
            .last_mut()
            .ok_or_else(|| internal_error!("call not initialized"))
    }

    /// The details of the innermost prepared call, if it has any.
    #[inline]
    pub(crate) fn top_details(&self) -> Option<&CallDetails> {
        let call = self.preparing.last()?;
        call.flags
            .contains(CallFlags::DETAILS)
            .then(|| self.details.last())
            .flatten()
    }

    /// The details of the innermost prepared call, made empty if it has none.
    pub(crate) fn top_details_mut(&mut self) -> Result<&mut CallDetails, NativeError> {
        let call = self
            .preparing
            .last_mut()
            .ok_or_else(|| internal_error!("call not initialized"))?;
        if !call.flags.contains(CallFlags::DETAILS) {
            call.flags.insert(CallFlags::DETAILS);
            self.details.push(CallDetails::default());
        }
        self.details
            .last_mut()
            .ok_or_else(|| internal_error!("call details"))
    }

    /// Finish preparing the innermost call: `Call` runs it, or it is abandoned.
    pub(crate) fn pop_call(&mut self) -> Result<(PreparedCall, Option<CallDetails>), NativeError> {
        let call = self
            .preparing
            .pop()
            .ok_or_else(|| internal_error!("call not initialized"))?;
        let details = match call.flags.contains(CallFlags::DETAILS) {
            true => self.details.pop(),
            false => None,
        };
        Ok((call, details))
    }

    /// A register was inserted at `insertion`: every prepared call whose
    /// window starts there or above moves up by one.
    pub(crate) fn shift_calls_from(&mut self, insertion: usize) {
        for call in &mut self.preparing {
            if call.base() >= insertion {
                call.shift_up();
            }
        }
    }

    /// Finish preparing the innermost call when the caller does not need
    /// its details (the interpreter loop enters only calls without any).
    #[inline]
    pub(crate) fn pop_plain(&mut self) -> Option<PreparedCall> {
        let call = self.preparing.pop()?;
        if call.flags.contains(CallFlags::DETAILS) {
            self.details.pop();
        }
        Some(call)
    }

    /// Abandon the prepared calls above the first `depth`.
    pub(crate) fn truncate_calls(&mut self, depth: usize) {
        while self.preparing.len() > depth {
            let _ = self.pop_call();
        }
    }

    /// Only the extension boundary needs a Vec; user calls never borrow this pool.
    pub(crate) fn native_arguments(&mut self) -> Vec<phera_value::Value> {
        self.native_arguments.pop().unwrap_or_default()
    }

    /// Clear PHP owners before retaining capacity for the next synchronous native invocation.
    pub(crate) fn recycle_native_arguments(&mut self, mut arguments: Vec<phera_value::Value>) {
        arguments.clear();
        self.native_arguments.push(arguments);
    }

    /// Native callbacks use Rust futures, so bound this nesting independently of PHP calls.
    pub(crate) fn enter_callback(&mut self) -> Result<(), NativeError> {
        if self.callback_depth >= 32 {
            return Err(NativeError::error("Maximum native callback depth exceeded"));
        }
        self.callback_depth += 1;
        Ok(())
    }

    /// Restore the native recursion budget after either callback outcome.
    pub(crate) fn leave_callback(&mut self) {
        self.callback_depth -= 1;
    }
}

#[cfg(test)]
impl CallStack {
    /// Stress tests ensure that the side-table entries of depths no frame
    /// uses hold nothing (closures and `$this` live in the registers, which
    /// returning empties).
    pub(crate) fn retained_exception_values(&self) -> usize {
        self.extras
            .iter()
            .skip(self.frames.len())
            .map(|extra| {
                usize::from(
                    extra.called.is_some()
                        || extra.property_hook.is_some()
                        || !extra.plain_return_can_release(),
                )
            })
            .sum()
    }

    /// A million sequential calls need no more side-table entries than a single call.
    pub(crate) fn recycled_count(&self) -> usize {
        self.extras.len().saturating_sub(self.frames.len())
    }

    /// How many frames the stack has held at once: the frame vector never
    /// shrinks, so deep recursion shows here after it has returned.
    pub(crate) fn frame_capacity(&self) -> usize {
        self.frames.capacity()
    }
}
