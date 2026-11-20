//! The full description of a callee: its name, its parameters and what runs.
//!
//! ```php
//! array_map('strtoupper', $words);   // a native function
//! $f = $point->move(...);            // a method, kept in a closure
//! ```
//!
//! Calls to PHP code are prepared without it (see `prepared_call`); a
//! [`Callee`] is made for everything the general path handles: native
//! functions, intrinsics, methods, callables resolved from values, and the
//! function a closure object wraps.

use std::rc::Rc;

use phera_extension_api::FunctionEntry;

/// A signature and target resolved once for a symbol in this request.
#[derive(Clone)]
pub(crate) struct Callee {
    pub(crate) argument_binding: crate::calls::callback_binding::ArgumentBinding,
    pub(crate) name: Rc<str>,
    pub(crate) parameters: Rc<[ParameterSpec]>,
    pub(crate) target: CallTarget,
}

/// Named argument routing and PHP parameter binding modifiers.
#[derive(Clone)]
pub(crate) struct ParameterSpec {
    pub(crate) name: String,
    /// Immutable declared type lets Send test its mask without reopening the function table.
    pub(crate) ty: Option<phera_bytecode::Type>,
    pub(crate) by_reference: bool,
    pub(crate) requires_reference: bool,
    pub(crate) variadic: bool,
}

/// User bytecode, a native entry, or an intrinsic resolved at the call site.
#[derive(Clone)]
pub(crate) enum CallTarget {
    /// Bound methods retain lexical and called scopes throughout argument evaluation.
    Method {
        method: Rc<crate::classes::Method>,
        context: crate::classes::MethodContext,
    },
    Closure(Rc<phera_value::Object>),
    Magic(Rc<super::magic_dispatch::MagicCall>),
    /// A declared PHP function.
    User(crate::code_table::FunctionIndex),
    Native(FunctionEntry),
    Intrinsic(crate::intrinsics::Intrinsic),
}

impl ParameterSpec {
    /// User signatures are retained once, so sends need no unit or name-table lookup.
    pub(crate) fn user(
        unit: &phera_bytecode::CompiledUnit,
        parameter: &phera_bytecode::Parameter,
    ) -> Self {
        Self {
            name: unit.names[parameter.name.index()].clone(),
            ty: parameter.ty.map(|ty| unit.types[ty.index()].clone()),
            by_reference: parameter.by_reference,
            requires_reference: parameter.by_reference,
            variadic: parameter.variadic,
        }
    }

    /// Native signatures have their own coercion policy at the extension boundary.
    pub(crate) fn native(parameter: &phera_extension_api::Param) -> Self {
        Self {
            name: parameter.name.into(),
            ty: None,
            by_reference: parameter.by_ref,
            requires_reference: parameter.requires_ref(),
            variadic: parameter.variadic,
        }
    }
}
