//! Objects whose unserialization did not complete get no destructor.
//!
//! ```php
//! class Guard { function __wakeup() { throw new Exception; } function __destruct() { echo "never\n"; } }
//! try { unserialize('O:5:"Guard":0:{}'); } catch (Exception $e) {}   // nothing printed
//! unserialize('O:5:"Guard":1:{s:1:"";}');                          // an error, nothing printed
//! ```
//!
//! PHP marks an object as destructed when its `__wakeup` or `__unserialize`
//! throws, when that call never runs because an earlier one threw, and when
//! the unserialization fails inside the object's own data
//! (`php_var_unserialize_destroy`, bug 72663). Objects without such a
//! method, and those whose call ran, are freed with their destructor.

use std::rc::Rc;

use phera_extension_api::{Context, Object};

use super::{Container, Decoder, Delayed};

impl Decoder<'_> {
    /// The unserialization failed: the objects still being read that wait
    /// for `__wakeup` or `__unserialize` will never get it.
    pub(super) fn abandon_unfinished(&self, context: &dyn Context) {
        for frame in &self.frames {
            match &frame.kind {
                Container::Properties(object)
                    if context.has_serialization_method(object, "__wakeup") =>
                {
                    object.mark_destructed();
                }
                Container::Unserialize(object) => object.mark_destructed(),
                _ => {}
            }
        }
    }
}

impl Delayed {
    /// The object this hook wakes, if it is `__wakeup` or `__unserialize`.
    pub(super) fn object(&self) -> Option<Rc<Object>> {
        match self {
            Delayed::Wakeup(object) | Delayed::Unserialize(object, _) => Some(object.clone()),
            Delayed::Legacy(_) => None,
        }
    }

    /// A hook before this one threw: its object is not woken.
    pub(super) fn abandon(&self) {
        if let Some(object) = self.object() {
            object.mark_destructed();
        }
    }
}
