//! `$this->p` by the slot its method knows, in the interpreter loop.
//!
//! ```php
//! final class Counter {
//!     private int $count = 0;
//!     public function add(): void { $this->count = $this->count + 1; }
//! }
//! ```
//!
//! The method's facts hold the slot of each such path (see
//! `crate::this_slots`), so a read or a write needs no property site: the
//! frame's `$this`, the object's properties and the slot. Anything the slot
//! cannot answer goes on to the property site and the VM: a lazy object, an
//! empty slot (unset, or not initialized yet), a PHP reference, a value the
//! declared type would convert.

use phera_bytecode::PathId;
use phera_value::{Object, Value};
use std::rc::Rc;

use crate::calls::execution_loop::{Machine, Position};
use crate::calls::prepared_call::CallKind;
use crate::this_slots::{SlotWrite, ThisSlot};

impl Machine<'_> {
    /// The value of `$this->p` when the method knows its slot.
    #[inline]
    pub(super) fn this_slot_value(&self, position: Position, path: PathId) -> Option<Value> {
        let (object, slot, _) = self.this_slot(position, path)?;
        object
            .with_properties(|properties| properties.slot(slot).map(|value| value.deref().clone()))
            .flatten()
    }

    /// Stores `value` in `$this->p` when the method knows its slot and the
    /// slot holds a value that is not a PHP reference; `false`, with nothing
    /// changed, otherwise.
    #[inline]
    pub(super) fn write_this_slot(&self, position: Position, path: PathId, value: &Value) -> bool {
        let Some((object, slot, write)) = self.this_slot(position, path) else {
            return false;
        };
        if value.is_reference() || !write.accepts(value) {
            return false;
        }
        // The old value is dropped once the borrow has ended.
        let replaced = object
            .update(|properties| match properties.slot_mut(slot) {
                Some(Value::Reference(_)) | None => None,
                Some(held) => Some(std::mem::replace(held, value.clone())),
            })
            .flatten();
        replaced.is_some()
    }

    /// The frame's `$this`, and the slot and write rule of `path` in it,
    /// when the method knows them.
    fn this_slot(
        &self,
        position: Position,
        path: PathId,
    ) -> Option<(&Rc<Object>, usize, SlotWrite)> {
        let frame = position.frame;
        if !matches!(frame.kind(), CallKind::Method | CallKind::Constructor) {
            return None;
        }
        let slots = self.code.facts(frame.function).this_slots.get_or_init(|| {
            crate::this_slots::this_slots(self.code, self.classes, frame.function).into()
        });
        let ThisSlot::Certain { slot, write } = *slots.get(path.index())? else {
            return None;
        };
        let object = match self.registers.window(frame.base().checked_sub(1)?, 1) {
            [Some(Value::Object(object))] => object,
            _ => return None,
        };
        if object.lazy().is_some() {
            return None;
        }
        Some((object, slot as usize, write))
    }
}
