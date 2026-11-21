//! Request-local class linking and method contexts keep shared bytecode free of PHP values.

pub(crate) mod class;
pub(crate) mod class_state;
mod class_table;
pub(crate) mod cloning;
mod declaration_errors;
pub(crate) mod enum_cases;
mod enum_methods;
pub(crate) mod enums;
mod inheritance_checks;
pub(crate) mod instantiation;
mod link_members;
mod linking;
mod magic;
pub(crate) mod magic_calls;
pub(crate) mod method_lookup;
pub(crate) mod method_table;
mod methods;
mod native;
pub(crate) mod operations;
pub(crate) mod parent_property_hooks;
mod property_hook_contracts;
mod property_hook_inheritance;
pub(crate) mod property_hooks;
mod protocol_methods;
mod signatures;
mod static_calls;
mod static_known;
mod trait_adaptations;
mod trait_defaults;
mod trait_members;
pub(crate) mod trait_methods;
mod trait_property_overrides;
pub(crate) mod worker_classes;

pub(crate) use class::{
    Class, ConstantState, Invocation, LinkedConstant, Method, MethodBody, MethodContext,
};
pub(crate) use class_table::{ClassDefinitions, ClassIndex, ClassTable};
