//! The start and end of a `foreach` over an array, in the interpreter loop.
//!
//! ```php
//! foreach ($rows as $row) { ... }   // IterStart, IterNext..., IterEnd
//! ```
//!
//! A `foreach` by value over an array iterates over the array as it was
//! when the loop started: the iterator keeps its own owner of it, in the
//! frame's slot for that loop (`FrameExtras::iterators`, one slot per
//! `foreach` of the function, by the compiler's number). The loop's
//! `IterNext` step already walks that owner; these two steps make and drop
//! it, as `Vm::iter_start` and `Vm::end_iterator` do.
//!
//! Everything else answers [`Step::Slow`] having changed nothing: objects
//! (their own iteration, hooks, `Traversable`), values that are not arrays
//! (PHP's warning), undefined variables and PHP references, a slot still
//! in use, a by-reference loop's state, and the guarded loop, where the
//! array let go of at the end could leave a destructor due.

use phera_bytecode::{IterId, Reg};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::frame::{IteratorState, TraversalMode};

impl Machine<'_> {
    /// `IterStart` of an array. Making the frame's side table marks the
    /// frame, so the step continues with the frame as it now is.
    pub(super) fn start_array_iteration<const GUARDED: bool>(
        &mut self,
        position: Position,
        iterator: IterId,
        source: Reg,
    ) -> Step {
        if GUARDED {
            return Step::Slow;
        }
        let slot = position.frame.base() + source.index();
        let [Some(Value::Array(array))] = self.registers.window(slot, 1) else {
            return Step::Slow;
        };
        let array = array.clone();
        let extras = self.stack.current_extras_mut();
        if !extras.iterators.is_free(iterator.0) {
            return Step::Slow;
        }
        extras.iterators.insert(
            iterator.0,
            IteratorState {
                snapshot: Some(array),
                traversal: None,
                traversal_mode: TraversalMode::Value,
                property_hooks: None,
                source: None,
                position: None,
                started: false,
            },
        );
        let Some(&frame) = self.stack.frames().last() else {
            return Step::Slow;
        };
        Step::Switched(Position {
            frame,
            ip: position.ip,
        })
    }

    /// `IterEnd` of a loop over an array snapshot.
    pub(super) fn end_array_iteration<const GUARDED: bool>(
        &mut self,
        position: Position,
        iterator: IterId,
    ) -> Step {
        if GUARDED || !position.frame.has_extras() {
            return Step::Slow;
        }
        let Some(extras) = self.stack.current_extras_present() else {
            return Step::Slow;
        };
        match extras.iterators.take_by_value(iterator.0) {
            Some(state) => {
                drop(state);
                Step::Done(position.ip)
            }
            None => Step::Slow,
        }
    }
}
