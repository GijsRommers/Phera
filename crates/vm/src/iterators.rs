//! Iterator state uses compiler-assigned dense slots rather than hashing IDs.

use crate::frame::IteratorState;

/// Each function allocates precisely the iterator count declared in bytecode.
pub(crate) struct Iterators {
    slots: Vec<Option<IteratorState>>,
}

impl Iterators {
    pub(crate) fn new(count: u16) -> Self {
        Self {
            slots: std::iter::repeat_with(|| None)
                .take(count as usize)
                .collect(),
        }
    }

    /// Recycled frames retain their backing allocation, but never live iterators.
    pub(crate) fn resize(&mut self, count: u16) {
        self.slots.clear();
        self.slots.resize_with(count as usize, || None);
    }

    pub(crate) fn insert(&mut self, index: u16, state: IteratorState) {
        if let Some(slot) = self.slots.get_mut(index as usize) {
            *slot = Some(state)
        }
    }

    pub(crate) fn remove(&mut self, index: &u16) -> Option<IteratorState> {
        self.slots.get_mut(*index as usize)?.take()
    }
}
