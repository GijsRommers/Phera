//! PHP iterator instructions instruction handlers, separate from dispatch.

use phera_bytecode::{IterId, PathId};
use phera_extension_api::NativeError;

use crate::Vm;

impl Vm {
    /// Implements PHP IterStartReference semantics outside the dispatch loop.
    pub(crate) fn start_reference_iterator(
        &mut self,
        iter: IterId,
        path: PathId,
    ) -> Result<(), NativeError> {
        let place = self.path(path)?;
        let value = self.read_place(&place, false)?;
        drop(value);
        self.iter_start_reference(iter, place)?;
        Ok(())
    }
    /// Implements PHP IterEnd semantics outside the dispatch loop.
    pub(crate) fn end_iterator(&mut self, iter: IterId) -> Result<(), NativeError> {
        if let Some(frame) = self.call_stack.frames.last_mut() {
            frame.iterators.remove(&iter.0);
        }
        Ok(())
    }
}
