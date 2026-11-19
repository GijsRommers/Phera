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
//! So a `WeakObject` keeps the object's id and serial number beside the
//! `Weak`. When the `Weak` is dead, it asks the request's id pool which
//! object now carries that id; the hand-over registered the new `Rc` there,
//! and the registration goes when that `Rc`'s lease on the id goes. The
//! serial number, never reused within a request, makes sure the object
//! found is the same PHP object and not a later one with the same id.

use std::rc::{Rc, Weak};

use crate::object::{Object, ObjectIds};

/// A weak handle on a PHP object; see the module documentation. The
/// default handle refers to nothing.
#[derive(Clone, Debug, Default)]
pub struct WeakObject {
    object: Weak<Object>,
    id: u32,
    serial: u64,
    pool: Weak<ObjectIds>,
}

impl WeakObject {
    pub fn new(object: &Rc<Object>) -> Self {
        let (serial, pool) = match &object.lease {
            Some(lease) => (lease.serial, Rc::downgrade(&lease.pool)),
            None => (0, Weak::new()),
        };
        WeakObject {
            object: Rc::downgrade(object),
            id: object.id,
            serial,
            pool,
        }
    }

    /// The object, while PHP still considers it alive.
    pub fn upgrade(&self) -> Option<Rc<Object>> {
        if let Some(object) = self.object.upgrade() {
            return Some(object);
        }
        let pool = self.pool.upgrade()?;
        let current = pool.incarnations.try_borrow().ok()?.get(&self.id)?.upgrade()?;
        (current.serial() == self.serial).then_some(current)
    }

    pub fn is_alive(&self) -> bool {
        self.upgrade().is_some()
    }

    /// Whether this handle is on `object` (in any of its incarnations).
    pub fn refers_to(&self, object: &Rc<Object>) -> bool {
        if self.serial == 0 {
            return std::ptr::eq(self.object.as_ptr(), Rc::as_ptr(object));
        }
        object.id == self.id && object.serial() == self.serial
    }
}

impl Object {
    /// The object's serial number: unique within its request, and kept by
    /// the object that waits for its destructor. 0 for an object made
    /// without an id pool.
    pub fn serial(&self) -> u64 {
        self.lease.as_ref().map_or(0, |lease| lease.serial)
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
