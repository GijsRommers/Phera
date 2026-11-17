//! Request-local call stack state kept separate from register execution.

use crate::calls::frame::Frame;

/// Owns the request state for call stack.
pub(crate) struct CallStack {
    /// Request-owned frames.
    frames: Vec<Frame>,
    /// Optional activation storage, indexed by frame depth.
    pub(crate) extras: Vec<super::frame_extras::FrameExtras>,
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

    /// Global aliases can refer to the retained top-level activation.
    pub(crate) fn main_mut(&mut self) -> Option<&mut Frame> {
        self.frames.first_mut()
    }

    /// Callback boundaries and the PHP frame limit use heap depth.
    pub(crate) fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Commit an activation without consuming Rust stack.
    pub(crate) fn push(&mut self, frame: Frame) {
        self.frames.push(frame);
    }

    /// Detach a copy of the activation before releasing its register window.
    pub(crate) fn pop(&mut self) -> Option<Frame> {
        self.frames.pop()
    }

    /// Exit releases optional owners and abandons pending windows.
    pub(crate) fn clear(&mut self) {
        self.preparing.clear();
        self.frames.clear();
        for extra in &mut self.extras {
            extra.clear();
        }
    }

    /// Traces borrow compact activations without retaining any code owner.
    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &Frame> + ExactSizeIterator {
        self.frames.iter()
    }

    /// Return releases only the optional state belonging to this depth.
    pub(crate) fn recycle(&mut self, frame: Frame) {
        self.extras[frame.depth].clear();
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

    /// Stress tests inspect reusable depth storage after iterative unwinding.
    pub(crate) fn recycled_count(&self) -> usize {
        self.extras.len().saturating_sub(self.frames.len())
    }
}
