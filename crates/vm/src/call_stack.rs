//! Request-local call stack state kept separate from register execution.

use phera_value::Value;

use crate::frame::Frame;

/// Owns the request state for call stack.
pub(crate) struct CallStack {
    /// Request-owned frames.
    pub(crate) frames: Vec<Frame>,
    /// Request-owned recycled frames.
    pub(crate) recycled_frames: Vec<Frame>,
    /// Request-owned argument buffers.
    pub(crate) argument_buffers: Vec<Vec<Option<Value>>>,
    /// Request-owned callback depth.
    pub(crate) callback_depth: usize,
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
