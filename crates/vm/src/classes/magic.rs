//! Linked magic handlers are consulted after ordinary lookup misses.
//!
//! `$object->missing()` uses the receiver's __call when it has one.

use std::{collections::HashMap, rc::Rc};

use super::Method;

/// Classes without magic methods carry only an empty option.
#[derive(Default)]
pub(crate) struct MagicMethods {
    pub(crate) call: Option<Rc<Method>>,
    pub(crate) call_static: Option<Rc<Method>>,
    pub(crate) clone: Option<Rc<Method>>,
    pub(crate) get: Option<Rc<Method>>,
    pub(crate) set: Option<Rc<Method>>,
    pub(crate) isset: Option<Rc<Method>>,
    pub(crate) unset: Option<Rc<Method>>,
    pub(crate) to_string: Option<Rc<Method>>,
    pub(crate) debug_info: Option<Rc<Method>>,
    pub(crate) invoke: Option<Rc<Method>>,
}

impl MagicMethods {
    pub(crate) fn has_properties(&self) -> bool {
        self.get.is_some() || self.set.is_some() || self.isset.is_some() || self.unset.is_some()
    }

    pub(super) fn link(methods: &HashMap<String, Rc<Method>>) -> Option<Box<Self>> {
        let linked = Self {
            call: methods.get("__call").cloned(),
            call_static: methods.get("__callstatic").cloned(),
            clone: methods.get("__clone").cloned(),
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
