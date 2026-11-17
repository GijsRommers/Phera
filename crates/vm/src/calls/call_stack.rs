//! Request-local call stack state kept separate from register execution.

use crate::calls::frame::Frame;

/// Stable allocations make a push/pop move one pointer instead of copying every frame buffer.
struct FrameOwner {
    frame: Box<Frame>,
}

/// Owns the request state for call stack.
pub(crate) struct CallStack {
    /// Request-owned frames.
    frames: Vec<FrameOwner>,
    /// Request-owned recycled frames.
    recycled_frames: Vec<FrameOwner>,
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
            recycled_frames: vec![],
            callback_depth: 0,
            argument_names: Vec::new(),
            native_arguments: Vec::new(),
            preparing: Vec::new(),
        }
    }
    /// The last activation owns the currently executing instruction.
    #[inline]
    pub(crate) fn current(&self) -> Option<&Frame> {
        self.frames.last().map(|owner| owner.frame.as_ref())
    }

    /// Only the active activation may change its instruction pointer.
    #[inline]
    pub(crate) fn current_mut(&mut self) -> Option<&mut Frame> {
        self.frames.last_mut().map(|owner| owner.frame.as_mut())
    }

    /// Global aliases can refer to the retained top-level activation.
    pub(crate) fn main_mut(&mut self) -> Option<&mut Frame> {
        self.frames.first_mut().map(|owner| owner.frame.as_mut())
    }

    /// Callback boundaries and the PHP frame limit use heap depth.
    pub(crate) fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Commit an activation without consuming Rust stack.
    pub(crate) fn push(&mut self, frame: impl Into<Box<Frame>>) {
        self.frames.push(FrameOwner {
            frame: frame.into(),
        });
    }

    /// Detach one activation before releasing its register window.
    pub(crate) fn pop(&mut self) -> Option<Box<Frame>> {
        self.frames.pop().map(|owner| owner.frame)
    }

    /// Exit discards preparations and all active activations.
    pub(crate) fn clear(&mut self) {
        self.preparing.clear();
        self.frames.clear();
    }

    /// Trace capture walks activations without copying their values.
    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &Frame> + ExactSizeIterator {
        self.frames.iter().map(|owner| owner.frame.as_ref())
    }

    /// Recycle allocations without retaining a second active PHP frame.
    pub(crate) fn recycle(&mut self, mut frame: Box<Frame>) {
        frame.closure = None;
        frame.method = None;
        frame.iterators.clear();
        self.recycled_frames.push(FrameOwner { frame });
    }

    /// Reuse storage only after its PHP owners have been released.
    pub(crate) fn take_recycled(&mut self) -> Option<Box<Frame>> {
        self.recycled_frames.pop().map(|owner| owner.frame)
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
    /// Stress tests check that recycled frames own buffers, but no live PHP values.
    pub(crate) fn retained_exception_values(&self) -> usize {
        self.recycled_frames
            .iter()
            .map(|owner| usize::from(owner.frame.closure.is_some()))
            .sum()
    }

    /// A million sequential calls need no more recycled frames than a single call.
    pub(crate) fn recycled_count(&self) -> usize {
        self.recycled_frames.len()
    }
}
