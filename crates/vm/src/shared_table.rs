//! A table that a snapshot and its forks hold together, and that a request
//! copies only when it changes it.
//!
//! ```php
//! // boot:    the application's files are loaded; the code table has 8,000 entries
//! // request: reads them (a call, a return), changes none  -> nothing is copied
//! // request: includes a file the boot did not load        -> the table is copied once
//! ```
//!
//! A [`SharedTable`] reads like a slice: `table[index]` is one load of the
//! table's address and one bounds check, the same instructions as a `Vec`
//! (the length sits next to the pointer, not behind it), so the read path
//! of the interpreter has no extra test for sharing. Changing it goes
//! through [`SharedTable::write`], which copies the whole table first if
//! another owner holds it, and which leaves room at the end, so that a
//! request that loads many files copies the table once and then grows it
//! like a vector.

use std::ops::Deref;
use std::rc::Rc;

/// A slice held by reference counting, copied on the first change.
pub(crate) struct SharedTable<T> {
    items: Rc<[T]>,
}

impl<T> Default for SharedTable<T> {
    fn default() -> Self {
        SharedTable {
            items: Rc::from([]),
        }
    }
}

impl<T> Deref for SharedTable<T> {
    type Target = [T];

    #[inline(always)]
    fn deref(&self) -> &[T] {
        &self.items
    }
}

impl<T> Clone for SharedTable<T> {
    /// Another owner of the same entries: no entry is copied.
    fn clone(&self) -> Self {
        SharedTable {
            items: self.items.clone(),
        }
    }
}

impl<T: Clone> SharedTable<T> {
    /// Entry `index`, for a change, with the table made the caller's own
    /// (and grown with `placeholder` entries to hold `index`) if it is not
    /// yet.
    pub(crate) fn write(&mut self, index: usize, placeholder: impl FnOnce() -> T) -> &mut T {
        let end = index + 1;
        if end > self.items.len() || Rc::get_mut(&mut self.items).is_none() {
            self.own(end, placeholder);
        }
        match Rc::get_mut(&mut self.items) {
            Some(items) => &mut items[index],
            None => unreachable!("the table was made unique above"),
        }
    }

    /// The entries from `start` to `end`, for a change, with the table made
    /// the caller's own and grown to hold them.
    pub(crate) fn write_range(
        &mut self,
        start: usize,
        end: usize,
        placeholder: impl FnOnce() -> T,
    ) -> &mut [T] {
        if end > self.items.len() || Rc::get_mut(&mut self.items).is_none() {
            self.own(end, placeholder);
        }
        match Rc::get_mut(&mut self.items) {
            Some(items) => &mut items[start..end],
            None => unreachable!("the table was made unique above"),
        }
    }

    /// A copy of the table that only this owner holds, with at least `end`
    /// entries. Growing leaves room for more, so that loading file after
    /// file does not copy the table each time.
    #[cold]
    #[inline(never)]
    fn own(&mut self, end: usize, placeholder: impl FnOnce() -> T) {
        let length = self.items.len();
        let wanted = if end > length {
            end + end / 8 + 16
        } else {
            length
        };
        let mut items = Vec::with_capacity(wanted);
        items.extend(self.items.iter().cloned());
        if wanted > length {
            // One placeholder, cloned: making one may allocate.
            items.resize(wanted, placeholder());
        }
        self.items = items.into();
    }
}

impl<T> SharedTable<T> {
    /// Whether this owner holds the table alone: it has made its own copy
    /// (or never shared it).
    #[cfg(debug_assertions)]
    pub(crate) fn is_unique(&self) -> bool {
        Rc::strong_count(&self.items) == 1
    }

    /// The bytes of the entries, which the table owns on the heap.
    #[cfg(debug_assertions)]
    pub(crate) fn heap_bytes(&self) -> usize {
        std::mem::size_of_val(&*self.items)
    }
}

#[cfg(test)]
mod tests {
    use super::SharedTable;

    /// A table holding `entries` first, with room for more after them.
    fn table(entries: &[u32]) -> SharedTable<u32> {
        let mut table = SharedTable::default();
        for (index, entry) in entries.iter().enumerate() {
            *table.write(index, || 0) = *entry;
        }
        table
    }

    /// A change by one owner is not seen by the other.
    #[test]
    fn a_change_copies_the_table_for_the_one_that_changes_it() {
        let snapshot = table(&[1, 2, 3]);
        let mut fork = snapshot.clone();
        assert!(!fork.is_unique(), "sharing copies nothing");
        *fork.write(1, || 0) = 20;
        assert_eq!(&snapshot[..3], &[1, 2, 3]);
        assert_eq!(&fork[..3], &[1, 20, 3]);
        assert!(fork.is_unique());
    }

    /// Growing fills with placeholders and leaves the other owner's table
    /// as it was.
    #[test]
    fn growing_pads_with_placeholders() {
        let snapshot = table(&[1, 2]);
        let before = snapshot.len();
        let mut fork = snapshot.clone();
        *fork.write(before + 5, || 7) = 9;
        assert_eq!(snapshot.len(), before);
        assert_eq!(&fork[..2], &[1, 2]);
        assert_eq!(fork[before + 3], 7);
        assert_eq!(fork[before + 5], 9);
    }

    /// Once a table is the caller's own, further changes copy nothing, and
    /// a range can be written in one go.
    #[test]
    fn an_owned_table_is_changed_in_place() {
        let mut owned = table(&[1, 2, 3, 4]);
        let other = owned.clone();
        owned.write_range(1, 3, || 0).fill(9);
        assert_eq!(&owned[..4], &[1, 9, 9, 4]);
        assert_eq!(&other[..4], &[1, 2, 3, 4]);
        assert!(owned.is_unique());
        *owned.write(0, || 0) = 5;
        assert_eq!(owned[0], 5);
    }
}
