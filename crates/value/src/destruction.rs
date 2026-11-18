//! Handing an object whose class has `__destruct` to the interpreter when
//! its last reference goes away.
//!
//! ```php
//! class Lock {
//!     public function __destruct() { echo "released\n"; }
//! }
//! function work() {
//!     $lock = new Lock;
//!     echo "working\n";
//! }                       // $lock's last reference ends here
//! work();
//! echo "done\n";          // prints: working, released, done
//! ```
//!
//! PHP runs the destructor at the moment the reference count reaches zero,
//! before the next statement. In Phera the count is an `Rc`, and Rust frees
//! the object in `Drop`. A `Drop` cannot run PHP code: it has no interpreter,
//! cannot wait for an asynchronous native call, and by the time it runs the
//! `Rc` is gone, so the object can no longer be passed as `$this`.
//!
//! So the drop hands the object over instead of freeing it:
//!
//! 1. An object whose class declares `__destruct` is created with its
//!    [`Destruction`] set to `Due`. Every other object stays `Never` and
//!    pays one comparison when it is freed, and nothing while it lives.
//! 2. When a `Due` object is dropped, its `Drop` moves the object's contents
//!    (class, properties, native state, and the lease on its id) into a new
//!    `Rc<Object>` marked `Done`, and pushes that onto the request's
//!    [`DoomedObjects`] queue. The id is still taken, so `spl_object_id`
//!    and `var_dump` see the same object.
//! 3. The interpreter looks at the queue after each instruction that can
//!    release a value, and calls `__destruct` on each queued object, in the
//!    order they were released, before it runs the next instruction.
//! 4. After the destructor the interpreter drops its handle. If the
//!    destructor stored `$this` somewhere, the object lives on; because it
//!    is `Done`, its next release frees it without another destructor call,
//!    as in PHP.
//!
//! Nothing here is `unsafe`: the object is rebuilt from moved fields rather
//! than revived. No destructor is lost (the queue owns the object until the
//! interpreter takes it) and none runs twice (only a `Due` object is queued,
//! and the queued copy is `Done`). At the end of a request the pool is
//! closed: objects freed after that are freed without their destructor, as
//! PHP frees what is left after it has run the destructors.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::object::{Object, ObjectIds};

/// Where an object stands with its destructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destruction {
    /// Its class has no `__destruct`.
    Never,
    /// `__destruct` runs when the last reference goes away.
    Due,
    /// `__destruct` has been called, or the request has ended; the next
    /// release frees the object.
    Done,
}

/// The objects of a request whose destructor is due, oldest first.
#[derive(Debug, Default)]
pub(crate) struct DoomedObjects {
    queue: RefCell<Vec<Rc<Object>>>,
    /// Set while the queue holds something: the interpreter tests this
    /// after instructions, so it is a plain flag rather than a borrow.
    waiting: Cell<bool>,
    /// The request has ended: objects are freed, not queued.
    closed: Cell<bool>,
}

impl DoomedObjects {
    fn push(&self, object: Rc<Object>) {
        // The queue is only ever borrowed for a push or a `take`, neither of
        // which can drop an object, so this borrow cannot be taken already.
        if let Ok(mut queue) = self.queue.try_borrow_mut() {
            queue.push(object);
            self.waiting.set(true);
        }
    }
}

impl Object {
    /// Whether the object waits for its destructor.
    pub fn destruction(&self) -> Destruction {
        self.destruction.get()
    }

    /// Marks a new object of a class that declares `__destruct`.
    pub fn expect_destructor(&self) {
        self.destruction.set(Destruction::Due);
    }

    /// Records that the destructor has run (or must no longer run).
    pub fn mark_destructed(&self) {
        self.destruction.set(Destruction::Done);
    }

    /// The contents of an object being dropped, moved into a new object
    /// that waits in the queue for its destructor.
    fn hand_over(&mut self) {
        let Some(lease) = self.lease.take() else {
            return;
        };
        let pool = lease.pool.clone();
        if pool.doomed.closed.get() {
            self.lease = Some(lease);
            return;
        }
        let doomed = Object {
            id: self.id,
            class: self.class.clone(),
            properties: RefCell::new(self.properties.take()),
            native: RefCell::new(self.native.take()),
            destruction: Cell::new(Destruction::Done),
            lease: Some(lease),
        };
        pool.doomed.push(Rc::new(doomed));
    }
}

impl Drop for Object {
    /// An object without a destructor is simply freed (its property table
    /// frees nested values without recursion, see `array::drop`).
    fn drop(&mut self) {
        if self.destruction.get() == Destruction::Due {
            self.hand_over();
        }
    }
}

impl ObjectIds {
    /// Whether some object waits for its destructor. Cheap: one flag.
    #[inline]
    pub fn has_doomed(&self) -> bool {
        self.doomed.waiting.get()
    }

    /// The objects waiting for their destructor, in the order they were
    /// released, leaving the queue empty.
    pub fn take_doomed(&self) -> Vec<Rc<Object>> {
        self.doomed.waiting.set(false);
        self.doomed
            .queue
            .try_borrow_mut()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }

    /// The request is over: from now on objects are freed without their
    /// destructor. Returns what was still queued, for the caller to free.
    pub fn close(&self) -> Vec<Rc<Object>> {
        self.doomed.closed.set(true);
        self.take_doomed()
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::rc::Rc;

    use crate::object::{ClassHandle, Object, ObjectIds};
    use crate::{ArrayKey, Value};

    use super::Destruction;

    struct Guard;

    impl ClassHandle for Guard {
        fn name(&self) -> &str {
            "Guard"
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn guard(ids: &Rc<ObjectIds>) -> Rc<Object> {
        let object = Object::allocate(ids, Rc::new(Guard));
        object.expect_destructor();
        Rc::new(object)
    }

    #[test]
    fn a_released_object_waits_with_its_id_and_properties() {
        let ids = ObjectIds::new();
        let object = guard(&ids);
        object.set(ArrayKey::str("name"), Value::from("lock"));
        drop(object);
        assert!(ids.has_doomed());
        assert_eq!(ids.live(), 1);
        let doomed = ids.take_doomed();
        assert!(!ids.has_doomed());
        assert_eq!(doomed.len(), 1);
        assert_eq!(doomed[0].id, 1);
        assert_eq!(doomed[0].destruction(), Destruction::Done);
        assert_eq!(
            doomed[0].get(&ArrayKey::str("name")),
            Some(Value::from("lock"))
        );
        drop(doomed);
        assert_eq!(ids.live(), 0);
        assert!(!ids.has_doomed());
    }

    #[test]
    fn objects_wait_in_the_order_they_were_released() {
        let ids = ObjectIds::new();
        let (first, second) = (guard(&ids), guard(&ids));
        drop(second);
        drop(first);
        let order: Vec<u32> = ids.take_doomed().iter().map(|object| object.id).collect();
        assert_eq!(order, vec![2, 1]);
    }

    #[test]
    fn a_closed_pool_frees_instead_of_queueing() {
        let ids = ObjectIds::new();
        let object = guard(&ids);
        assert!(ids.close().is_empty());
        drop(object);
        assert!(!ids.has_doomed());
        assert_eq!(ids.live(), 0);
    }

    /// A chain of 100,000 objects with destructors, released at once, is
    /// queued without recursion: each object is queued whole, with its
    /// properties, and the next one is released only when the interpreter
    /// drops the queued one.
    #[test]
    fn a_long_chain_with_destructors_does_not_recurse() {
        let ids = ObjectIds::new();
        let mut head = Value::Null;
        for _ in 0..100_000 {
            let node = guard(&ids);
            node.set(ArrayKey::str("next"), head);
            head = Value::Object(node);
        }
        drop(head);
        let mut destructed = 0;
        loop {
            let doomed = ids.take_doomed();
            if doomed.is_empty() {
                break;
            }
            destructed += doomed.len();
        }
        assert_eq!(destructed, 100_000);
        assert_eq!(ids.live(), 0);
    }
}
