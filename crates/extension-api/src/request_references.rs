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
