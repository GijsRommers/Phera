//! Linking records which operations the engine may send to an object.
//!
//! ```php
//! interface Items extends Iterator {}
//! // Classes implementing Items also participate in foreach.
//! ```

use super::{Class, Method};
use std::{collections::HashMap, rc::Rc};

#[derive(Clone, Copy, Default)]
pub(crate) struct ClassOperations(u16);

/// Linked once, so a dimension access does not search a subclass method table.
#[derive(Clone, Copy)]
pub(crate) enum NativeDimension {
    Get = 1 << 5,
    Exists = 1 << 6,
    Set = 1 << 7,
    Unset = 1 << 8,
}

impl NativeDimension {
    const ALL: [Self; 4] = [Self::Get, Self::Exists, Self::Set, Self::Unset];

    fn method(self) -> &'static str {
        match self {
            Self::Get => "offsetget",
            Self::Exists => "offsetexists",
            Self::Set => "offsetset",
            Self::Unset => "offsetunset",
        }
    }

    pub(crate) fn bit(self) -> u16 {
        self as u16
    }
}

impl ClassOperations {
    pub(crate) const TRAVERSABLE: u16 = 1;
    pub(crate) const ITERATOR: u16 = 2;
    pub(crate) const AGGREGATE: u16 = 4;
    pub(crate) const ARRAY_ACCESS: u16 = 8;
    pub(crate) const COUNTABLE: u16 = 16;
    pub(crate) const DIMENSIONS: u16 = 1 << 9;

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

    pub(crate) fn with_native_dimensions(
        mut self,
        hooks: Option<&phera_extension_api::ClassHooks>,
        methods: &HashMap<String, Rc<super::Method>>,
    ) -> Self {
        for kind in NativeDimension::ALL {
            self.0 &= !kind.bit();
        }
        let dimensions = hooks.and_then(|hooks| hooks.dimensions.as_ref());
        if dimensions.is_some() || self.has(Self::ARRAY_ACCESS) {
            self.0 |= Self::DIMENSIONS;
        }
        let Some(dimensions) = dimensions else {
            return self;
        };
        let overridden = |name| {
            methods
                .get(name)
                .is_some_and(|method| matches!(method.body, super::MethodBody::User(_)))
        };
        if !dimensions
            .fallback_if_overridden
            .iter()
            .any(|name| overridden(*name))
        {
            for kind in NativeDimension::ALL {
                if !overridden(kind.method()) {
                    self.0 |= kind.bit();
                }
            }
        }
        self
    }

    pub(crate) fn has(self, flag: u16) -> bool {
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
        binary: own.binary.or(base.binary),
        unary: own.unary.or(base.unary),
        cast: own.cast.or(base.cast),
        forbid_serialization: own.forbid_serialization || base.forbid_serialization,
        redact_trace_arguments: own.redact_trace_arguments || base.redact_trace_arguments,
        fork: own.fork.or(base.fork),
        properties: own.properties.or(base.properties),
        property_access: own.property_access.or(base.property_access),
        iterator: own.iterator.or(base.iterator),
        iterator_by_reference: if own.iterator.is_some() {
            own.iterator_by_reference
        } else {
            base.iterator_by_reference
        },
        iterator_fallback_if_overridden: if own.iterator.is_some() {
            own.iterator_fallback_if_overridden
        } else {
            base.iterator_fallback_if_overridden
        },
        dimensions: own.dimensions.or(base.dimensions),
        count: own.count.or(base.count),
        forward_method: own.forward_method.or(base.forward_method),
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
    if iterator_methods
        .iter()
        .chain(hooks.iterator_fallback_if_overridden)
        .any(|&name| {
            methods.get(name).is_some_and(|method| {
                parent
                    .methods
                    .get(name)
                    .is_none_or(|inherited| !Rc::ptr_eq(inherited, method))
            })
        })
    {
        Some(Rc::new(phera_extension_api::ClassHooks {
            iterator: None,
            iterator_by_reference: None,
            ..**hooks
        }))
    } else {
        Some(hooks.clone())
    }
}
