//! PHP iterator instructions instruction handlers, separate from dispatch.

use phera_bytecode::{IterId, PathId};
use phera_extension_api::NativeError;

use crate::Vm;

impl Vm {
    /// Implements PHP IterStartReference semantics outside the dispatch loop.
    pub(crate) fn start_reference_iterator(
        &mut self,
        iterator: IterId,
        path: PathId,
    ) -> Result<(), NativeError> {
        let place = self.path(path)?;
        let value = self.read_place(&place, crate::access_mode::Missing::Warn)?;
        drop(value);
        self.iter_start_reference(iterator, place)?;
        Ok(())
    }

    /// Implements PHP IterEnd semantics outside the dispatch loop.
    pub(crate) fn end_iterator(&mut self, iterator: IterId) -> Result<(), NativeError> {
        if let Some(frame) = self.call_stack.current_mut() {
            frame.iterators.remove(&iterator.0);
        }
        Ok(())
    }
}
