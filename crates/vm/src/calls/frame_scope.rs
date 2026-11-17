//! Method frames store `$this` in registers and scopes as class indexes.
//!
//! In `$object->f()`, a hidden register owns the receiver while the activation
//! stores only that register's address. Bound Closure objects keep their own owner.

use crate::{Vm, calls::frame::Frame, classes::MethodContext};
use phera_value::{Object, Value};
use std::rc::Rc;

/// Only method frames use this numeric context; it owns no PHP object or class.
#[derive(Clone, Copy)]
pub(crate) struct FrameMethod {
    pub(crate) receiver: Option<usize>,
    pub(crate) scope: crate::classes::class_table::ClassIndex,
    pub(crate) called: crate::classes::class_table::ClassIndex,
    pub(crate) invocation: crate::classes::Invocation,
}

impl Vm {
    /// Transfer the prepared receiver owner into a hidden register above the argument window.
    pub(crate) fn install_frame_method(&mut self, frame: &Frame, context: Option<MethodContext>) {
        self.call_stack.extras[frame.depth].method = context.map(|context| {
            let receiver = context.receiver.map(|receiver| {
                let slot = self.registers.len();
                self.registers.grow_to(slot + 1);
                self.registers.window_mut(slot, 1)[0] = Some(Value::Object(receiver));
                slot
            });
            FrameMethod {
                receiver,
                scope: context.scope,
                called: context.called,
                invocation: context.invocation,
            }
        });
    }

    /// PHP reads of `$this` borrow its hidden owner before producing a PHP value.
    pub(crate) fn frame_receiver(&self, context: &FrameMethod) -> Option<&Rc<Object>> {
        match self
            .registers
            .window(context.receiver?, 1)
            .first()?
            .as_ref()?
        {
            Value::Object(object) => Some(object),
            _ => None,
        }
    }

    /// A newly created Closure must outlive the frame, so it materializes its own bound owner.
    pub(crate) fn current_method_context(&self) -> Option<MethodContext> {
        let context = self.call_stack.extras[self.current()?.depth]
            .method
            .as_ref()?;
        Some(MethodContext {
            receiver: self.frame_receiver(context).cloned(),
            scope: context.scope,
            called: context.called,
            invocation: context.invocation,
        })
    }
}
