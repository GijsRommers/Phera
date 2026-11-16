//! Request-local call stack state kept separate from register execution.

use phera_value::Value;

use crate::frame::Frame;

/// Owns the request state for call stack.
pub(crate) struct CallStack {
    /// Request-owned frames.
    frames: Vec<Frame>,
    /// Request-owned recycled frames.
    recycled_frames: Vec<Frame>,
    /// Request-owned argument buffers.
    argument_buffers: Vec<Vec<Option<Value>>>,
    /// Request-owned callback depth.
    callback_depth: usize,
}

impl CallStack {
    pub(crate) fn new() -> Self {
        Self {
            frames: vec![],
            recycled_frames: vec![],
            argument_buffers: vec![],
            callback_depth: 0,
        }
    }
}

impl CallStack {
    #[inline]
    pub(crate) fn current(&self) -> Option<&Frame> {
        self.frames.last()
    }

    #[inline]
    pub(crate) fn current_mut(&mut self) -> Option<&mut Frame> {
        self.frames.last_mut()
    }

    pub(crate) fn main_mut(&mut self) -> Option<&mut Frame> {
        self.frames.first_mut()
    }

    pub(crate) fn depth(&self) -> usize {
        self.frames.len()
    }

    pub(crate) fn push(&mut self, frame: Frame) {
        self.frames.push(frame);
    }

    pub(crate) fn pop(&mut self) -> Option<Frame> {
        self.frames.pop()
    }

    pub(crate) fn clear(&mut self) {
        self.frames.clear();
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &Frame> {
        self.frames.iter()
    }

    /// Recycle allocations without retaining a second active PHP frame.
    pub(crate) fn recycle(&mut self, mut frame: Frame) {
        frame.method = None;
        self.recycled_frames.push(frame);
    }

    pub(crate) fn take_recycled(&mut self) -> Option<Frame> {
        self.recycled_frames.pop()
    }

    pub(crate) fn recycle_arguments(&mut self, arguments: Vec<Option<Value>>) {
        self.argument_buffers.push(arguments);
    }

    pub(crate) fn take_arguments(&mut self) -> Vec<Option<Value>> {
        self.argument_buffers.pop().unwrap_or_default()
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

    pub(crate) fn leave_callback(&mut self) {
        self.callback_depth -= 1;
    }
}
