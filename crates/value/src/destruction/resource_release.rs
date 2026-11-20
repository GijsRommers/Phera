//! Releasing `$file = null` hands the wrapper close to the interpreter.

use std::rc::{Rc, Weak};

use super::{Doomed, ObjectIds};

#[derive(Debug)]
pub struct ReleasedResource {
    pub kind: &'static str,
    pub id: i64,
}

/// A wrapper's native handle owns this guard until close or last release.
pub struct ResourceRelease {
    ids: Weak<ObjectIds>,
    released: Option<ReleasedResource>,
}

impl ResourceRelease {
    pub fn new(ids: &Rc<ObjectIds>, kind: &'static str, id: i64) -> Self {
        ids.doomed.due.set(ids.doomed.due.get() + 1);
        Self {
            ids: Rc::downgrade(ids),
            released: Some(ReleasedResource { kind, id }),
        }
    }

    /// An explicit close takes care of this callback itself.
    pub fn cancel(&mut self) {
        if self.released.take().is_some()
            && let Some(ids) = self.ids.upgrade()
        {
            ids.doomed.due.set(ids.doomed.due.get().saturating_sub(1));
        }
    }
}

impl Drop for ResourceRelease {
    fn drop(&mut self) {
        let Some(released) = self.released.take() else {
            return;
        };
        let Some(ids) = self.ids.upgrade() else {
            return;
        };
        ids.doomed.due.set(ids.doomed.due.get().saturating_sub(1));
        if !ids.doomed.closed.get() {
            ids.doomed.push(Doomed::Resource(Box::new(released)));
        }
    }
}
