//! Compiler-known static calls avoid materializing class and method PHP strings.

use super::Invocation;
use crate::{Vm, calls::call_state::CallTarget};
use phera_bytecode::{ClassBinding, SymbolId};
use phera_extension_api::NativeError;

impl Vm {
    /// Compiler-resolved symbols avoid repeating name conversion during static preparation.
    pub(crate) fn prepare_known_static_call(
        &mut self,
        class: SymbolId,
        method: SymbolId,
        binding: ClassBinding,
    ) -> Result<(), NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("known static frame"))?;
        let method_name = self.code.loaded(frame.function).symbols[method.index()];
        let index = self.class_index(class)?;
        let resolved = self
            .resolve_method(index, method_name, Invocation::Method)?
            .ok_or_else(|| {
                NativeError::error(format!(
                    "Call to undefined method {}::{}()",
                    self.classes.at(index).name,
                    self.current().unwrap().unit(&self.code).symbols[method.index()].written
                ))
            })?;
        let context = self
            .current()
            .and_then(|frame| self.call_stack.extras[frame.depth].method);
        let receiver = if resolved.method.is_static {
            None
        } else {
            context
                .as_ref()
                .and_then(|context| self.frame_receiver(context))
                .filter(|object| {
                    self.classes
                        .index_for_object(object)
                        .is_some_and(|receiver| self.classes.at(receiver).includes(index))
                })
                .cloned()
        };
        self.prepare_resolved_method(index, receiver, resolved, Invocation::Method)?;
        if binding == ClassBinding::Forward
            && let Some(context) = context
        {
            let pending = self.call_stack.pending_mut()?;
            if let CallTarget::Method {
                context: target, ..
            } = &mut pending.callee.target
            {
                target.called = context.called;
            }
        }
        Ok(())
    }
}
