//! Linking records which operations the engine may send to an object.
//!
//! ```php
//! interface Items extends Iterator {}
//! // Classes implementing Items also participate in foreach.
//! ```

use super::{Class, Method};
use std::{collections::HashMap, rc::Rc};

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

/// Subclasses keep native handlers unless a native subclass replaces an operation.
pub(super) fn inherit_hooks(
    entry: Option<&phera_extension_api::ClassHooks>,
    parent: Option<&Class>,
) -> Option<Rc<phera_extension_api::ClassHooks>> {
    let base = parent.and_then(|class| class.hooks.as_ref());
    let Some(own) = entry else {
        return base.cloned();
    };
    let base = base.map_or(phera_extension_api::ClassHooks::EMPTY, |hooks| **hooks);
    Some(Rc::new(phera_extension_api::ClassHooks {
        clone: own.clone.or(base.clone),
        compare: own.compare.or(base.compare),
        properties: own.properties.or(base.properties),
        property_access: own.property_access.or(base.property_access),
        iterator: own.iterator.or(base.iterator),
        dimensions: own.dimensions.or(base.dimensions),
        count: own.count.or(base.count),
    }))
}

/// PHP uses a native iterator until a subclass replaces a method of its protocol.
pub(super) fn user_hooks(
    methods: &HashMap<String, Rc<Method>>,
    parent: Option<&Class>,
    operations: ClassOperations,
) -> Option<Rc<phera_extension_api::ClassHooks>> {
    let parent = parent?;
    let hooks = parent.hooks.as_ref()?;
    if hooks.iterator.is_none() {
        return Some(hooks.clone());
    }
    let iterator_methods: &[&str] = if operations.has(ClassOperations::AGGREGATE) {
        &["getiterator"]
    } else if operations.has(ClassOperations::ITERATOR) {
        &["rewind", "valid", "current", "key", "next"]
    } else {
        &[]
    };
    if iterator_methods.iter().any(|&name| {
        methods.get(name).is_some_and(|method| {
            parent
                .methods
                .get(name)
                .is_none_or(|inherited| !Rc::ptr_eq(inherited, method))
        })
    }) {
        Some(Rc::new(phera_extension_api::ClassHooks {
            iterator: None,
            ..**hooks
        }))
    } else {
        Some(hooks.clone())
    }
}
