//! PHP iterator instructions instruction handlers, separate from dispatch.

use phera_bytecode::{IterId, PathId, Reg};
use phera_extension_api::NativeError;

use crate::Vm;

impl Vm {
    /// Implements PHP IterStartReference semantics outside the dispatch loop.
    pub(crate) fn start_reference_iterator(
        &mut self,
        iterator: IterId,
        path: PathId,
        subject: Reg,
    ) -> Result<(), NativeError> {
        let place = self.path(path)?;
        let value = self.read_place(&place, crate::access_mode::Missing::Warn)?;
        drop(value);
        let reference = self.reference(&place)?;
        self.set(subject, reference);
        self.iter_start_reference(iterator, crate::AccessPath::local(subject))?;
        Ok(())
    }

    /// Implements PHP IterEnd semantics outside the dispatch loop.
    pub(crate) fn end_iterator(&mut self, iterator: IterId) -> Result<(), NativeError> {
        if let Some(frame) = self.call_stack.current().copied() {
            self.call_stack.extras[frame.depth]
                .iterators
                .remove(&iterator.0);
        }
        Ok(())
    }
}
