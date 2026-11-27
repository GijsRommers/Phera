//! What a call needs to know about the function it enters.
//!
//! ```php
//! function scale(int $value, float $by = 2.0, &$log = null): float { ... }
//! ```
//!
//! For `scale` the record says: 3 parameters, 1 required, parameter 0 typed
//! `int` and parameter 1 `float` (one bit each, and the type's built-in
//! mask for a one-bit check of an argument), parameter 2 by reference, the
//! return type `float`, how many registers a frame needs. It is made once,
//! when the file is loaded, and read on every call, so it is kept small.
//! It also holds the function's instructions, shared with the compiled
//! file, so that a call or a return reaches them from here.

use std::sync::Arc;

pub(crate) use phera_bytecode::FunctionShape;
use phera_bytecode::{Code, FunctionSignature, TypeMask};

use crate::code_table::{FunctionIndex, UnitIndex};

/// What a call needs to know about the function it enters, read on every
/// call, so it is kept small and next to nothing else.
#[derive(Clone)]
pub(crate) struct FunctionFacts {
    /// The function's instructions, which the interpreter loop runs.
    pub(crate) instructions: Code,
    pub(crate) unit: UnitIndex,
    /// `None` for a file's top-level code, else its number within the file.
    pub(crate) local: Option<u32>,
    /// The file's top-level code, from which `CallDirect` counts.
    pub(crate) unit_first: FunctionIndex,
    pub(crate) registers: u16,
    pub(crate) parameters: u16,
    /// Parameters before the last one without a default value.
    pub(crate) required: u16,
    /// One bit per parameter with a declared type (the first 64).
    typed: u64,
    /// One bit per parameter that takes a reference or is variadic (the
    /// first 64): a `Send` to it is not a plain copy.
    special: u64,
    /// One bit per parameter that takes a reference (the first 64).
    by_reference: u64,
    /// The declared built-in types of each parameter, for the one-bit check.
    pub(crate) parameter_masks: Arc<[TypeMask]>,
    /// The declared return type's built-in types, when there is a return type.
    pub(crate) return_mask: Option<TypeMask>,
    pub(crate) shape: FunctionShape,
    /// Whether this function has a Tier-2-clean loop or self-recursive body,
    /// computed once at unit load time so non-candidate routes pay zero Tier-2
    /// lookup cost.
    pub(crate) fast_candidate: bool,
    /// The class that declares this function as a method: a method frame's
    /// class scope.
    pub(crate) class: Option<crate::classes::ClassIndex>,
}

impl FunctionFacts {
    /// Declared-type bits for a caller checking positional arguments in order.
    pub(crate) fn typed_parameters(&self) -> u64 {
        self.typed
    }

    /// Whether parameter `index` has a declared type.
    pub(crate) fn is_typed(&self, index: usize) -> bool {
        index < 64 && self.typed & (1 << index) != 0
    }

    /// Whether parameter `index` is declared, by reference and not variadic.
    pub(crate) fn takes_reference(&self, index: usize) -> bool {
        index < self.parameters as usize && index < 64 && self.by_reference & (1 << index) != 0
    }

    /// Whether every declared parameter takes a copy of its argument (none
    /// is by reference or variadic), so that no argument can be one a
    /// reference was expected for.
    pub(crate) fn takes_values_only(&self) -> bool {
        self.special == 0 && self.parameters <= 64
    }

    /// Whether parameter `index` is declared and takes a copy of its
    /// argument: it is neither by reference nor variadic.
    pub(crate) fn takes_value(&self, index: usize) -> bool {
        index < self.parameters as usize && index < 64 && self.special & (1 << index) == 0
    }

    /// The facts at a number of a function the request has not loaded: no
    /// unit, no code. They are never run (see `crate::code_table::CodeTable`).
    pub(crate) fn placeholder() -> Self {
        Self::bind(
            (UnitIndex(u32::MAX), FunctionIndex(u32::MAX), None),
            FunctionSignature {
                registers: 0,
                parameters: 0,
                required: 0,
                typed: 0,
                special: 0,
                by_reference: 0,
                parameter_masks: std::sync::Arc::from([]),
                return_mask: None,
                shape: phera_bytecode::FunctionShape::Plain,
            },
            Default::default(),
        )
    }

    pub(crate) fn bind(
        (unit, unit_first, local): (UnitIndex, FunctionIndex, Option<u32>),
        signature: FunctionSignature,
        instructions: Code,
    ) -> Self {
        Self {
            instructions,
            unit,
            local,
            unit_first,
            registers: signature.registers,
            parameters: signature.parameters,
            required: signature.required,
            typed: signature.typed,
            special: signature.special,
            by_reference: signature.by_reference,
            parameter_masks: signature.parameter_masks,
            return_mask: signature.return_mask,
            shape: signature.shape,
            fast_candidate: false,
            class: None,
        }
    }
}
