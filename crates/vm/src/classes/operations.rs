//! Linking records which operations the engine may send to an object.
//!
//! ```php
//! interface Items extends Iterator {}
//! // Classes implementing Items also participate in foreach.
//! ```

use super::Class;
use std::rc::Rc;

#[derive(Clone, Copy, Default)]
pub(crate) struct ClassOperations(u8);

impl ClassOperations {
    pub(crate) const TRAVERSABLE: u8 = 1;
    pub(crate) const ITERATOR: u8 = 2;
    pub(crate) const AGGREGATE: u8 = 4;
    pub(crate) const ARRAY_ACCESS: u8 = 8;
    pub(crate) const COUNTABLE: u8 = 16;

    pub(crate) fn link(name: &str, parent: Option<&Class>, interfaces: &[Rc<Class>]) -> Self {
        let mut flags = match name {
            "Traversable" => Self::TRAVERSABLE,
            "Iterator" => Self::ITERATOR,
            "IteratorAggregate" => Self::AGGREGATE,
            "ArrayAccess" => Self::ARRAY_ACCESS,
            "Countable" => Self::COUNTABLE,
            _ => 0,
        };
        if let Some(parent) = parent {
            flags |= parent.operations.0;
        }
        for interface in interfaces {
            flags |= interface.operations.0;
        }
        Self(flags)
    }

    pub(crate) fn has(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}
