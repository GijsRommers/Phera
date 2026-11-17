//! Request-local class linking and method contexts keep shared bytecode free of PHP values.

pub(crate) mod class;
mod class_table;
mod declaration_errors;
mod inheritance_checks;
pub(crate) mod instantiation;
mod linking;
pub(crate) mod method_cache;
mod method_results;
mod methods;
mod native;
mod signatures;
mod static_calls;
mod static_known;
pub(crate) mod static_sites;

pub(crate) use class::{
    Class, ConstantState, Invocation, LinkedConstant, Method, MethodBody, MethodContext,
};
pub(crate) use class_table::ClassTable;

pub(crate) use methods::MethodPreparation;
