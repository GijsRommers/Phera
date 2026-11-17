//! PHP's internal array pointer, separate from foreach positions.

use super::{ArrayKey, INVALID, PhpArray};
use crate::value::Value;

impl PhpArray {
    fn valid_pos(&self) -> Option<usize> {
        let position = self.pos as usize;
        self.storage.value(position).map(|_| position)
    }

    pub fn current(&self) -> Option<&Value> {
        self.storage.value(self.pos as usize)
    }
    pub fn key(&self) -> Option<ArrayKey> {
        self.entry_at(self.pos as usize).map(|(key, _)| key)
    }

    /// Advancing from the invalid state restarts at the first entry, as before.
    pub fn advance(&mut self) {
        self.pos = match self.valid_pos() {
            Some(position) => self.next_position(position),
            None => self.first_position(),
        }
        .map_or(INVALID, |position| position as u32);
    }

    /// Moving before the beginning invalidates the pointer until advance/reset.
    pub fn rewind_back(&mut self) {
        self.pos = self
            .valid_pos()
            .and_then(|position| {
                (0..position)
                    .rev()
                    .find(|&index| self.storage.value(index).is_some())
            })
            .map_or(INVALID, |position| position as u32);
    }

    pub fn reset(&mut self) {
        self.pos = self
            .first_position()
            .map_or(INVALID, |position| position as u32);
    }
    pub fn end(&mut self) {
        self.pos = self
            .last_position()
            .map_or(INVALID, |position| position as u32);
    }
}
