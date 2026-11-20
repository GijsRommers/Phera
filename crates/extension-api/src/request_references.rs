//! A reference can keep an otherwise unreachable PHP array alive.
//!
//! ```php
//! $a = [];
//! $a['self'] =& $a;
//! unset($a);
//! ```
//!
//! The request remembers weak handles to such cells. After PHP shutdown,
//! dropping the request state severs the remaining edges.

use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

use crate::{RequestState, Value};

type Cell = RefCell<Value>;

#[derive(Default)]
struct CycleReferences(HashMap<*const Cell, Weak<Cell>>);

impl RequestState {
    /// Remember a reference cell that can keep an array alive by pointing
    /// back into it. Ordinary unreachable cells remain free to drop.
    pub fn track_cycle_reference(&mut self, cell: &Rc<RefCell<Value>>) {
        let references = &mut self.get_or_default::<CycleReferences>().0;
        if references.len() == references.capacity() {
            references.retain(|_, entry| entry.strong_count() != 0);
        }
        references
            .entry(Rc::as_ptr(cell))
            .or_insert_with(|| Rc::downgrade(cell));
    }
}

impl Drop for CycleReferences {
    fn drop(&mut self) {
        for cell in self.0.values().filter_map(Weak::upgrade) {
            let value = cell
                .try_borrow_mut()
                .ok()
                .map(|mut value| std::mem::take(&mut *value));
            drop(value);
        }
    }
}

/// A fork of a snapshot watches its own copies of the cells (see
/// `phera_value::Fork`); a cell nothing reaches any more is left out.
pub fn fork_cycle_references(
    state: &dyn std::any::Any,
    fork: &mut phera_value::Fork<'_>,
) -> Option<Box<dyn std::any::Any>> {
    let references = state.downcast_ref::<CycleReferences>()?;
    let mut copies = HashMap::new();
    for cell in references.0.values().filter_map(Weak::upgrade) {
        let copy = fork.reference(&cell);
        copies.insert(Rc::as_ptr(&copy), Rc::downgrade(&copy));
    }
    Some(Box::new(CycleReferences(copies)))
}

/// Registers [`fork_cycle_references`] for the cycle bookkeeping.
pub fn register_cycle_fork(registry: &mut crate::Registry) {
    registry.fork_state::<CycleReferences>(fork_cycle_references);
}
