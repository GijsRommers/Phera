//! Linked magic handlers are consulted after ordinary lookup misses.
//!
//! `$object->missing()` uses the receiver's __call when it has one.

use std::rc::Rc;

use foldhash::HashMap;

use super::Method;

/// Classes without magic methods carry only an empty option.
#[derive(Default)]
pub(crate) struct MagicMethods {
    pub(crate) call: Option<Rc<Method>>,
    pub(crate) call_static: Option<Rc<Method>>,
    pub(crate) clone: Option<Rc<Method>>,
    /// Runs when the last reference to an instance goes away; see
    /// `crate::destructors`.
    pub(crate) destruct: Option<Rc<Method>>,
    pub(crate) get: Option<Rc<Method>>,
    pub(crate) set: Option<Rc<Method>>,
    pub(crate) isset: Option<Rc<Method>>,
    pub(crate) unset: Option<Rc<Method>>,
    pub(crate) to_string: Option<Rc<Method>>,
    pub(crate) debug_info: Option<Rc<Method>>,
    pub(crate) invoke: Option<Rc<Method>>,
}

impl MagicMethods {
    pub(super) fn is_linked_magic(name: &str) -> bool {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "__call"
                | "__callstatic"
                | "__clone"
                | "__destruct"
                | "__get"
                | "__set"
                | "__isset"
                | "__unset"
                | "__tostring"
                | "__debuginfo"
                | "__invoke"
        )
    }

    pub(crate) fn has_properties(&self) -> bool {
        self.get.is_some() || self.set.is_some() || self.isset.is_some() || self.unset.is_some()
    }

    pub(super) fn link(methods: &HashMap<String, Rc<Method>>) -> Option<Box<Self>> {
        let linked = Self {
            call: methods.get("__call").cloned(),
            call_static: methods.get("__callstatic").cloned(),
            clone: methods.get("__clone").cloned(),
            destruct: methods.get("__destruct").cloned(),
            get: methods.get("__get").cloned(),
            set: methods.get("__set").cloned(),
            isset: methods.get("__isset").cloned(),
            unset: methods.get("__unset").cloned(),
            to_string: methods.get("__tostring").cloned(),
            debug_info: methods.get("__debuginfo").cloned(),
            invoke: methods.get("__invoke").cloned(),
        };
        if linked.call.is_some()
            || linked.call_static.is_some()
            || linked.invoke.is_some()
            || linked.clone.is_some()
            || linked.destruct.is_some()
            || linked.debug_info.is_some()
            || linked.to_string.is_some()
            || linked.has_properties()
        {
            Some(Box::new(linked))
        } else {
            None
        }
    }
}
