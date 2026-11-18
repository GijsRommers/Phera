//! The internal pointer of `current()`, `key()`, `next()`, `prev()`,
//! `reset()` and `end()`. It is separate from the positions foreach keeps.
//!
//! ```php
//! $a = [1, 2, 3];
//! current($a);    // 1: a new array points at its first entry
//! unset($a[0]);
//! current($a);    // 2: a pointer on a hole reads the next live entry
//! end($a);
//! next($a);       // false: past the end
//! next($a);       // still past the end; it does not start over
//! $a[] = 4;
//! current($a);    // 4: the new entry is where the pointer was
//! ```
//!
//! As in php-src, the pointer is a slot position, and any position after
//! the last live slot means "past the end".

use super::{ArrayKey, PhpArray};
use crate::value::Value;

impl PhpArray {
    /// Position of the entry the pointer reads: the first live slot at or
    /// after it.
    fn pointed(&self) -> Option<usize> {
        self.live_from(self.pos as usize)
    }

    pub(super) fn set_pointer(&mut self, position: usize) {
        self.pos = u32::try_from(position).expect("array position fits u32");
    }

    /// The pointer position meaning "past the end": the slot the next
    /// append fills, so that the appended entry becomes current, as in PHP.
    fn past_the_end(&self) -> usize {
        self.storage.len()
    }

    pub fn current(&self) -> Option<&Value> {
        self.storage.value(self.pointed()?)
    }

    pub fn key(&self) -> Option<ArrayKey> {
        self.entry_at(self.pointed()?).map(|(key, _)| key)
    }

    /// `next()`. Past the end it stays there.
    pub fn advance(&mut self) {
        if let Some(position) = self.pointed() {
            let next = self.next_position(position);
            self.set_pointer(next.unwrap_or_else(|| self.past_the_end()));
        }
    }

    /// `prev()`. Before the first entry it goes past the end, from where
    /// neither `prev()` nor `next()` moves it.
    pub fn rewind_back(&mut self) {
        if self.pointed().is_some() {
            let position = self.pos as usize;
            let previous = (0..position)
                .rev()
                .find(|&index| self.storage.value(index).is_some());
            self.set_pointer(previous.unwrap_or_else(|| self.past_the_end()));
        }
    }

    pub fn reset(&mut self) {
        let first = self.first_position();
        self.set_pointer(first.unwrap_or_else(|| self.past_the_end()));
    }

    pub fn end(&mut self) {
        let last = self.last_position();
        self.set_pointer(last.unwrap_or_else(|| self.past_the_end()));
    }

    /// Where the pointer goes when the holes are dropped: to the same entry,
    /// or, on a hole, to the live entry that followed it, which is the
    /// number of live slots before it. A pointer past the end keeps its
    /// number, as php-src's rehash does; so an append after compaction does
    /// not become current there.
    pub(super) fn pointer_after_compaction(&self) -> usize {
        let before = self.pos as usize;
        if self.pointed().is_none() {
            return before;
        }
        let (list, entries) = self.storage.parts();
        let live_list = list.iter().take(before).filter(|value| value.is_some());
        let live_map = entries
            .iter()
            .take(before)
            .filter(|entry| entry.value.is_some());
        live_list.count() + live_map.count()
    }
}
