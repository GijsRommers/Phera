//! Request-local call stack state kept separate from register execution.

use phera_value::Value;

use crate::frame::Frame;

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
    /// Request-owned argument buffers.
    argument_buffers: Vec<Vec<Option<Value>>>,
    /// Request-owned callback depth.
    callback_depth: usize,
    /// Bound targets reuse their Rc allocation after releasing all object receivers.
    bound_callees: Vec<std::rc::Rc<crate::call_state::Callee>>,
    /// One spare method context avoids an allocation on sequential method calls.
    method_context: Option<Box<crate::classes::MethodContext>>,
}

impl CallStack {
    pub(crate) fn new() -> Self {
        Self {
            frames: vec![],
            recycled_frames: vec![],
            argument_buffers: vec![],
            callback_depth: 0,
            bound_callees: Vec::new(),
            method_context: None,
        }
    }
}

impl CallStack {
    #[inline]
    pub(crate) fn current(&self) -> Option<&Frame> {
        self.frames.last().map(|owner| owner.frame.as_ref())
    }

    #[inline]
    pub(crate) fn current_mut(&mut self) -> Option<&mut Frame> {
        self.frames.last_mut().map(|owner| owner.frame.as_mut())
    }

    pub(crate) fn main_mut(&mut self) -> Option<&mut Frame> {
        self.frames.first_mut().map(|owner| owner.frame.as_mut())
    }

    pub(crate) fn depth(&self) -> usize {
        self.frames.len()
    }

    pub(crate) fn push(&mut self, frame: impl Into<Box<Frame>>) {
        self.frames.push(FrameOwner {
            frame: frame.into(),
        });
    }

    pub(crate) fn pop(&mut self) -> Option<Box<Frame>> {
        self.frames.pop().map(|owner| owner.frame)
    }

    pub(crate) fn clear(&mut self) {
        self.frames.clear();
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &Frame> + ExactSizeIterator {
        self.frames.iter().map(|owner| owner.frame.as_ref())
    }

    /// Recycle allocations without retaining a second active PHP frame.
    pub(crate) fn recycle(&mut self, mut frame: Box<Frame>) {
        frame.arguments.clear();
        frame.closure = None;
        if let Some(mut context) = frame.method.take() {
            context.receiver = None;
            self.method_context = Some(context);
        }
        self.recycled_frames.push(FrameOwner { frame });
    }

    pub(crate) fn take_recycled(&mut self) -> Option<Box<Frame>> {
        self.recycled_frames.pop().map(|owner| owner.frame)
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

impl CallStack {
    /// Reuse a uniquely owned bound call; symbol-cache entries never enter this pool.
    pub(crate) fn bound_callee(
        &mut self,
        value: crate::call_state::Callee,
    ) -> std::rc::Rc<crate::call_state::Callee> {
        if let Some(mut storage) = self.bound_callees.pop()
            && let Some(target) = std::rc::Rc::get_mut(&mut storage)
        {
            *target = value;
            return storage;
        }
        std::rc::Rc::new(value)
    }

    /// Vacant storage retains no object; keeping a receiver here would delay id reuse.
    pub(crate) fn recycle_bound_callee(
        &mut self,
        mut storage: std::rc::Rc<crate::call_state::Callee>,
    ) {
        use crate::call_state::CallTarget;
        if !matches!(
            &storage.target,
            CallTarget::Method { .. } | CallTarget::Constructed(_) | CallTarget::Closure(_)
        ) {
            return;
        }
        if let Some(target) = std::rc::Rc::get_mut(&mut storage) {
            target.target = CallTarget::Vacant;
            self.bound_callees.push(storage);
        }
    }

    /// Mirrors Box::new(context), replacing the contents of a receiver-free spare.
    pub(crate) fn method_context(
        &mut self,
        value: crate::classes::MethodContext,
    ) -> Box<crate::classes::MethodContext> {
        if let Some(mut storage) = self.method_context.take() {
            *storage = value;
            storage
        } else {
            Box::new(value)
        }
    }
}

#[cfg(test)]
impl CallStack {
    /// Stress tests check that recycled frames own buffers, but no live PHP values.
    pub(crate) fn retained_exception_values(&self) -> usize {
        self.recycled_frames
            .iter()
            .map(|owner| owner.frame.arguments.len() + owner.frame.calls.len())
            .sum()
    }

    /// A million sequential calls need no more recycled frames than a single call.
    pub(crate) fn recycled_count(&self) -> usize {
        self.recycled_frames.len()
    }
}
