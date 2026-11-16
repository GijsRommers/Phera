//! Request-local class linking and method contexts keep shared bytecode free of PHP values.

mod class;
mod class_table;
mod inheritance_checks;
mod instantiation;
mod linking;
mod methods;

pub(crate) use class::{Class, Invocation, Method, MethodBody, MethodContext};
pub(crate) use class_table::ClassTable;
