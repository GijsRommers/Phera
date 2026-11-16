//! Request-local class linking and method contexts keep shared bytecode free of PHP values.

pub(crate) mod class;
mod class_table;
mod inheritance_checks;
mod instantiation;
mod linking;
mod methods;
mod native;

pub(crate) use class::{
    Class, ConstantState, Invocation, LinkedConstant, Method, MethodBody, MethodContext,
};
pub(crate) use class_table::ClassTable;
