//! Small iterative path workspaces with a heap fallback for unusually deep paths.
//!
//! Eight entries cover ordinary PHP array access. A spill vector preserves
//! unbounded iterative traversal without unsafe code or Rust recursion.

/// A stack whose first eight elements live inline.
pub(crate) struct InlineStack<T> {
    entries: [Option<T>; 8],
    spill: Vec<T>,
    length: usize,
}

impl<T> InlineStack<T> {
    pub(crate) fn new() -> Self {
        Self {
            entries: std::array::from_fn(|_| None),
            spill: Vec::new(),
            length: 0,
        }
    }

    pub(crate) fn push(&mut self, value: T) {
        if let Some(slot) = self.entries.get_mut(self.length) {
            *slot = Some(value)
        } else {
            self.spill.push(value)
        }
        self.length += 1;
    }

    pub(crate) fn pop(&mut self) -> Option<T> {
        self.length = self.length.checked_sub(1)?;
        if self.length < self.entries.len() {
            self.entries[self.length].take()
        } else {
            self.spill.pop()
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.length
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &T> {
        self.entries
            .iter()
            .filter_map(Option::as_ref)
            .chain(self.spill.iter())
    }
}
