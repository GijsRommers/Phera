//! The open containers of `json_encode([[1], [2]])` stay on an explicit stack.

use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

use phera_extension_api::value::{PropertyScope, split_property_key};
use phera_extension_api::{ArrayKey, Object, PhpArray, Value};

use crate::recursion::Serializing;

pub(super) enum Owner {
    Array(Rc<PhpArray>),
    Object(Rc<Object>, Rc<PhpArray>),
}

impl Owner {
    pub(super) fn identity(&self) -> usize {
        match self {
            Self::Array(array) => Rc::as_ptr(array) as usize,
            Self::Object(object, _) => Rc::as_ptr(object) as usize,
        }
    }

    fn entries(&self) -> &PhpArray {
        match self {
            Self::Array(array) | Self::Object(_, array) => array,
        }
    }
}

pub(super) struct Container {
    owner: Owner,
    next: Option<usize>,
    pub(super) object: bool,
    pub(super) level: usize,
    pub(super) wrote_entry: bool,
}

impl Container {
    pub(super) fn new(owner: Owner, object: bool, level: usize) -> Self {
        Self {
            next: owner.entries().first_position(),
            owner,
            object,
            level,
            wrote_entry: false,
        }
    }

    pub(super) fn next_entry(&mut self) -> Option<(ArrayKey, Value)> {
        let entries = self.owner.entries();
        while let Some(position) = self.next {
            self.next = entries.next_position(position);
            let (key, value) = entries.entry_at(position)?;
            if matches!(self.owner, Owner::Object(..))
                && let Some(name) = key.as_str_bytes()
                && split_property_key(name).1 != PropertyScope::Public
            {
                continue;
            }
            return Some((key, value.clone()));
        }
        None
    }
}

pub(super) enum Work {
    Value(Value, usize),
    Container(Container),
    ReleaseReference(Rc<RefCell<Value>>),
    ReleaseSerializer(Serializing),
}

impl Work {
    pub(super) fn value(value: Value, level: usize) -> Self {
        Self::Value(value, level)
    }

    fn identity(&self) -> Option<usize> {
        match self {
            Self::Container(container) => Some(container.owner.identity()),
            Self::ReleaseReference(reference) => Some(Rc::as_ptr(reference) as usize),
            _ => None,
        }
    }
}

/// Flat data needs only its one open container. Nested inputs spill onto the heap.
#[derive(Default)]
pub(super) struct Stack {
    top: Option<Work>,
    parents: Vec<Work>,
    older: BTreeSet<usize>,
}

/// Search the nearest parents directly. Beyond this depth, an ordered set
/// prevents repeated searches of the whole ancestry of a deeply nested value.
const NEARBY_PARENTS: usize = 32;

impl Stack {
    pub(super) fn push(&mut self, work: Work) {
        if let Some(parent) = self.top.replace(work) {
            if let Some(identity) = self.oldest_nearby_identity() {
                self.older.insert(identity);
            }
            self.parents.push(parent);
        }
    }

    pub(super) fn pop(&mut self) -> Option<Work> {
        let top = self.top.take()?;
        self.top = self.parents.pop();
        if let Some(identity) = self.oldest_nearby_identity() {
            self.older.remove(&identity);
        }
        Some(top)
    }

    fn oldest_nearby_identity(&self) -> Option<usize> {
        let oldest = self.parents.len().checked_sub(NEARBY_PARENTS)?;
        self.parents.get(oldest)?.identity()
    }

    pub(super) fn contains(&self, identity: usize) -> bool {
        self.top
            .iter()
            .chain(self.parents.iter().rev().take(NEARBY_PARENTS))
            .any(|work| work.identity() == Some(identity))
            || self.older.contains(&identity)
    }
}
