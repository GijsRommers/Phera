//! Where a static property access names its class and property.
//!
//! ```php
//! Post::$count++;          // named in the source: the class's symbol and the name
//! $class::$count++;        // known at run time: registers holding the strings
//! ```
//!
//! Both forms reach the same handlers through [`static_base`]; only turning
//! the class and name into strings or a class differs.

use phera_bytecode::{NameId, PathBase, Reg, SymbolId};

/// The class of a static property access.
#[derive(Clone, Copy)]
pub(crate) enum StaticClass {
    /// A register holding a class name or an object.
    Register(Reg),
    /// A class the source names.
    Named(SymbolId),
}

/// The property name of a static property access.
#[derive(Clone, Copy)]
pub(crate) enum StaticName {
    /// A register holding the name.
    Register(Reg),
    /// A name written in the source.
    Named(NameId),
}

/// The class and name of a path that starts at a static property.
pub(crate) fn static_base(base: PathBase) -> Option<(StaticClass, StaticName)> {
    match base {
        PathBase::StaticProperty { class, name, .. } => {
            Some((StaticClass::Register(class), StaticName::Register(name)))
        }
        PathBase::NamedStaticProperty { class, name, .. } => {
            Some((StaticClass::Named(class), StaticName::Named(name)))
        }
        _ => None,
    }
}
