//! Request-local call stack state kept separate from register execution.

use crate::calls::frame::Frame;
use crate::calls::frame_extras::FrameExtras;

/// The running PHP functions of a request, innermost last, and the calls
/// whose arguments are being evaluated.
pub(crate) struct CallStack {
    frames: Vec<Frame>,
    /// Optional per-frame state, indexed by frame depth.
    extras: Vec<FrameExtras>,
    /// What `extras` returns for a frame that stores nothing there.
    empty: FrameExtras,
    /// Argument evaluation and native callbacks share this request-wide preparation stack.
    pub(crate) preparing: Vec<crate::calls::call_state::PendingCall>,
    /// Native callbacks have an independent bound because they await Rust futures.
    callback_depth: usize,
    pub(crate) argument_names: Vec<Vec<(usize, phera_value::PhpString)>>,
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
            argument_names: Vec::new(),
            native_arguments: Vec::new(),
            preparing: Vec::new(),
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
            .is_some_and(|call| call.window.base >= base)
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
        self.preparing.clear();
        while self.pop().is_some() {}
    }

    /// Drop the prepared calls whose windows start at or above `base`: a
    /// frame left while its arguments were being evaluated.
    pub(crate) fn abandon_calls_from(&mut self, base: usize) {
        while self.has_calls_from(base) {
            self.preparing.pop();
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

    /// Every Send must have a resolved target and reserved window.
    pub(crate) fn pending(
        &self,
    ) -> Result<&crate::calls::call_state::PendingCall, phera_extension_api::NativeError> {
        self.preparing
            .last()
            .ok_or_else(|| internal_error!("call not initialized"))
    }

    /// Routing changes only the most recently reserved window.
    pub(crate) fn pending_mut(
        &mut self,
    ) -> Result<&mut crate::calls::call_state::PendingCall, phera_extension_api::NativeError> {
        self.preparing
            .last_mut()
            .ok_or_else(|| internal_error!("call not initialized"))
    }

    /// Call consumes exactly one preparation; outer argument lists remain reserved.
    pub(crate) fn take_pending(
        &mut self,
    ) -> Result<crate::calls::call_state::PendingCall, phera_extension_api::NativeError> {
        self.preparing
            .pop()
            .ok_or_else(|| internal_error!("call not initialized"))
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
    pub(crate) fn enter_callback(&mut self) -> Result<(), phera_extension_api::NativeError> {
        if self.callback_depth >= 32 {
            return Err(phera_extension_api::NativeError::error(
                "Maximum native callback depth exceeded",
            ));
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
    /// Stress tests ensure inactive depths retain no closure owners.
    pub(crate) fn retained_exception_values(&self) -> usize {
        self.extras
            .iter()
            .map(|extra| usize::from(extra.closure.is_some()))
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
