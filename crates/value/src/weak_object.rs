//! A handle that sees an object for as long as PHP's weak references do.
//!
//! ```php
//! class D { function __destruct() { global $ref; var_dump($ref->get() === $this); } }
//! $d = new D;
//! $ref = WeakReference::create($d);
//! unset($d);              // bool(true): the destructor still sees the object
//! var_dump($ref->get());  // NULL: now it is gone
//! ```
//!
//! PHP clears weak references when it frees an object, which is after its
//! destructor, and not at all if the destructor stored `$this` somewhere.
//! In Phera the last `Rc` of an object with a destructor is gone when the
//! destructor is due: the object's contents move into a new `Rc` that waits
//! for the destructor (see `crate::destruction`). A plain `Weak` would then
//! report the object as gone too early.
//!
//! So a `WeakObject` keeps the object's id and identity beside the `Weak`.
//! When the `Weak` is dead, it asks the request's id pool which object now
//! carries that id; the hand-over registered the new `Rc` there, with the
//! identity of the object it took over from, and the registration goes
//! when the new `Rc` is freed.
//!
//! An object's identity is the address of the allocation it was created
//! in. The handle's `Weak` keeps that allocation reserved (an `Rc`'s memory
//! is only given back when its last `Weak` goes), so no later object can
//! have the same identity while the handle exists.

use std::rc::{Rc, Weak};

use crate::object::{Object, ObjectIds};

/// A weak handle on a PHP object; see the module documentation. The
/// default handle refers to nothing.
#[derive(Clone, Debug, Default)]
pub struct WeakObject {
    object: Weak<Object>,
    id: u32,
    /// The object's [`Object::identity`].
    origin: usize,
    pool: Weak<ObjectIds>,
}

impl WeakObject {
    pub fn new(object: &Rc<Object>) -> Self {
        WeakObject {
            object: Rc::downgrade(object),
            id: object.id,
            origin: object.identity(),
            pool: object.pool.as_deref().map_or_else(Weak::new, Rc::downgrade),
        }
    }

    /// The object, while PHP still considers it alive.
    pub fn upgrade(&self) -> Option<Rc<Object>> {
        if let Some(object) = self.object.upgrade() {
            return Some(object);
        }
        let pool = self.pool.upgrade()?;
        let incarnations = pool.incarnations.try_borrow().ok()?;
        let (origin, current) = incarnations.get(&self.id)?;
        if *origin != self.origin {
            return None;
        }
        current.upgrade()
    }

    pub fn is_alive(&self) -> bool {
        self.upgrade().is_some()
    }

    /// Whether this handle is on `object` (in any of its incarnations).
    pub fn refers_to(&self, object: &Rc<Object>) -> bool {
        object.identity() == self.origin
    }
}

impl Object {
    /// Which PHP object this is: the address of the allocation it was
    /// created in, unique among the objects alive (and among those a
    /// `WeakObject` still refers to). An object that took over another's
    /// contents for its destructor has the identity of the one it took
    /// over from.
    pub fn identity(&self) -> usize {
        if self.incarnated
            && let Some(pool) = &self.pool
            && let Ok(incarnations) = pool.incarnations.try_borrow()
            && let Some((origin, _)) = incarnations.get(&self.id)
        {
            return *origin;
        }
        self.address()
    }

    /// Where this object is in memory.
    pub(crate) fn address(&self) -> usize {
        std::ptr::from_ref(self) as usize
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::rc::Rc;

    use super::WeakObject;
    use crate::object::{ClassHandle, Object, ObjectIds};

    struct Guard;

    impl ClassHandle for Guard {
        fn name(&self) -> &str {
            "Guard"
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn object(ids: &Rc<ObjectIds>, destructor: bool) -> Rc<Object> {
        let object = Object::allocate(ids, Rc::new(Guard));
        if destructor {
            object.expect_destructor();
        }
        Rc::new(object)
    }

    #[test]
    fn an_object_waiting_for_its_destructor_is_still_seen() {
        let ids = ObjectIds::new();
        let target = object(&ids, true);
        let weak = WeakObject::new(&target);
        drop(target);
        let doomed = ids.take_doomed();
        let seen = weak.upgrade().expect("still alive while queued");
        assert!(Rc::ptr_eq(&seen, &doomed[0]));
        assert!(weak.refers_to(&doomed[0]));
        drop((seen, doomed));
        assert!(!weak.is_alive());
    }

    #[test]
    fn a_later_object_with_the_same_id_is_not_seen() {
        let ids = ObjectIds::new();
        let first = object(&ids, true);
        let weak = WeakObject::new(&first);
        drop(first);
        drop(ids.take_doomed());
        let second = object(&ids, true);
        assert_eq!(second.id, 1);
        drop(second);
        let doomed = ids.take_doomed();
        assert_eq!(doomed.len(), 1);
        assert!(weak.upgrade().is_none());
        assert!(!weak.refers_to(&doomed[0]));
    }

    #[test]
    fn an_object_without_destructor_goes_at_once() {
        let ids = ObjectIds::new();
        let target = object(&ids, false);
        let weak = WeakObject::new(&target);
        assert!(weak.refers_to(&target));
        drop(target);
        assert!(!weak.is_alive());
    }
}
