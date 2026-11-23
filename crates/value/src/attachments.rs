//! Values that a `WeakMap` keeps for an object, stored on the object itself.
//!
//! ```php
//! class D { function __destruct() { echo "value freed\n"; } }
//! $map = new WeakMap;
//! $key = new stdClass;
//! $map[$key] = new D;
//! unset($key);          // "value freed": the entry goes with its key
//! echo count($map);     // 0
//! ```
//!
//! PHP removes a `WeakMap` entry, and releases its value, at the moment
//! the key object is freed (after the key's destructor, and not at all if
//! the destructor stored `$this`). Rust frees an object in `Drop`, which
//! cannot call into the map. So the map keeps only weak handles on its keys
//! and stores each value on the key object, tagged with the map's identity
//! (see `Object::identity`): freeing the key frees its values in the same moment.
//!
//! The collector counts the attached owner but only follows the value
//! when both the map and key are reachable independently of that entry.

use std::cell::Cell;

use crate::object::Object;
use crate::value::Value;

/// The attached values of one object, by owner (a map's identity, see `Object::identity`).
pub(crate) type Attachments = Cell<Option<Box<Vec<(usize, Value)>>>>;

impl Object {
    /// The value attached for `owner`, if any.
    pub fn attachment(&self, owner: usize) -> Option<Value> {
        self.with_attachments(|attached| {
            attached
                .iter()
                .find(|(candidate, _)| *candidate == owner)
                .map(|(_, value)| value.clone())
        })
    }

    /// Changes the value attached for `owner` in place; `None` if there is
    /// none.
    pub fn update_attachment<R>(
        &self,
        owner: usize,
        change: impl FnOnce(&mut Value) -> R,
    ) -> Option<R> {
        self.with_attachments(|attached| {
            attached
                .iter_mut()
                .find(|(candidate, _)| *candidate == owner)
                .map(|(_, value)| change(value))
        })
    }

    /// Attaches `value` for `owner`; returns the value it replaces, for the
    /// caller to drop.
    pub fn attach(&self, owner: usize, value: Value) -> Option<Value> {
        self.with_attachments(|attached| {
            match attached
                .iter_mut()
                .find(|(candidate, _)| *candidate == owner)
            {
                Some((_, slot)) => Some(std::mem::replace(slot, value)),
                None => {
                    attached.push((owner, value));
                    None
                }
            }
        })
    }

    /// Removes the value attached for `owner`; returns it, for the caller
    /// to drop.
    pub fn detach(&self, owner: usize) -> Option<Value> {
        self.with_attachments(|attached| {
            let index = attached
                .iter()
                .position(|(candidate, _)| *candidate == owner)?;
            Some(attached.swap_remove(index).1)
        })
    }

    /// Whether any value is attached, for the cycle collector.
    pub(crate) fn has_attachments(&self) -> bool {
        let attached = self.attachments.take();
        let any = attached.is_some();
        self.attachments.set(attached);
        any
    }

    /// Calls `visit` with each map identity and its value, for the cycle collector.
    pub(crate) fn visit_attachments(&self, mut visit: impl FnMut(usize, &Value)) {
        if let Some(attached) = self.attachments.take() {
            for (owner, value) in attached.iter() {
                visit(*owner, value);
            }
            self.attachments.set(Some(attached));
        }
    }

    /// Empties the attachments, for breaking a garbage cycle apart and for
    /// the end of a request; returns them, for the caller to drop.
    pub fn take_attachments(&self) -> Vec<Value> {
        self.attachments
            .take()
            .map(|attached| attached.into_iter().map(|(_, value)| value).collect())
            .unwrap_or_default()
    }

    /// Runs `use_them` on the attachments. They are taken out of the cell
    /// for the call and put back after, so no borrow is held; nothing in
    /// `use_them` reaches this object's attachments again.
    pub(crate) fn with_attachments<R>(
        &self,
        use_them: impl FnOnce(&mut Vec<(usize, Value)>) -> R,
    ) -> R {
        let mut attached = self.attachments.take().unwrap_or_default();
        let result = use_them(&mut attached);
        if !attached.is_empty() {
            self.attachments.set(Some(attached));
        }
        result
    }
}
