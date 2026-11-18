//! Request-local class linking and method contexts keep shared bytecode free of PHP values.

pub(crate) mod class;
mod class_table;
mod declaration_errors;
mod inheritance_checks;
pub(crate) mod instantiation;
mod linking;
pub(crate) mod method_lookup;
pub(crate) mod method_table;
mod methods;
mod native;
pub(crate) mod operations;
mod protocol_methods;
mod signatures;
mod static_calls;
mod static_known;

pub(crate) use class::{
    Class, ConstantState, Invocation, LinkedConstant, Method, MethodBody, MethodContext,
};
pub(crate) use class_table::{ClassIndex, ClassTable};
