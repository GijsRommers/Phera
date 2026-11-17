//! Iterator state uses compiler-assigned dense slots rather than hashing IDs.

use crate::calls::frame::IteratorState;

/// Each function allocates precisely the iterator count declared in bytecode.
pub(crate) struct Iterators {
    slots: Vec<Option<IteratorState>>,
}

impl Iterators {
    /// Foreach slots grow only when the activation needs more iterator storage.
    pub(crate) fn new(count: u16) -> Self {
        Self {
            slots: std::iter::repeat_with(|| None)
                .take(count as usize)
                .collect(),
        }
    }

    /// Recycled activations retain capacity across different function signatures.
    pub(crate) fn resize(&mut self, count: u16) {
        self.clear();
        self.slots.resize_with(count as usize, || None);
    }

    /// Return releases every iterator owner without freeing its backing storage.
    pub(crate) fn clear(&mut self) {
        self.slots.fill_with(|| None);
    }

    /// A whole-subject assignment restarts live iterators sharing that reference cell.
    pub(crate) fn restart_for(
        &mut self,
        subject: &phera_value::Value,
        slots: &[Option<phera_value::Value>],
    ) {
        let phera_value::Value::Reference(subject) = subject else {
            return;
        };
        for state in self.slots.iter_mut().flatten() {
            let Some(source) = &state.source else {
                continue;
            };
            let Ok(phera_bytecode::PathBase::Register(register)) = source.base() else {
                continue;
            };
            if let Some(Some(phera_value::Value::Reference(reference))) =
                slots.get(register.index())
                && std::rc::Rc::ptr_eq(subject, reference)
            {
                state.started = false;
                state.position = None;
            }
        }
    }

    /// A compiler-assigned slot replaces any traversal already stored there.
    pub(crate) fn insert(&mut self, index: u16, state: IteratorState) {
        if let Some(slot) = self.slots.get_mut(index as usize) {
            *slot = Some(state)
        }
    }

    /// Ending foreach transfers its remaining owners out of the activation.
    pub(crate) fn remove(&mut self, index: &u16) -> Option<IteratorState> {
        self.slots.get_mut(*index as usize)?.take()
    }

    /// Borrow a snapshot cursor in place instead of moving the whole state per element.
    pub(crate) fn get_mut(&mut self, index: u16) -> Option<&mut IteratorState> {
        self.slots.get_mut(index as usize)?.as_mut()
    }
}
