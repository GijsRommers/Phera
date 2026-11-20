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

use phera_bytecode::{Code, Function, Type, TypeMask};

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
    pub(crate) parameter_masks: Box<[TypeMask]>,
    /// The declared return type's built-in types, when there is a return type.
    pub(crate) return_mask: Option<TypeMask>,
    pub(crate) shape: FunctionShape,
    /// The class that declares this function as a method: a method frame's
    /// class scope.
    pub(crate) class: Option<crate::classes::ClassIndex>,
}

/// Whether entering a function is just moving arguments into place.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FunctionShape {
    /// At most 64 parameters, none of them variadic.
    Plain,
    /// A variadic parameter or more than 64 parameters: the general entry
    /// collects or checks what the bit masks cannot describe.
    General,
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

    /// Whether parameter `index` is declared and takes a copy of its
    /// argument: it is neither by reference nor variadic.
    pub(crate) fn takes_value(&self, index: usize) -> bool {
        index < self.parameters as usize && index < 64 && self.special & (1 << index) == 0
    }

    pub(crate) fn new(
        place: (UnitIndex, FunctionIndex, Option<u32>),
        body: &Function,
        types: &[Type],
    ) -> Self {
        let (unit, unit_first, local) = place;
        let mask_of = |ty: phera_bytecode::TypeId| types[ty.index()].mask;
        let masks = body
            .parameters
            .iter()
            .map(|parameter| parameter.ty.map_or(TypeMask::MIXED, mask_of))
            .collect();
        let bits = |test: fn(&phera_bytecode::Parameter) -> bool| {
            body.parameters
                .iter()
                .take(64)
                .enumerate()
                .filter(|(_, parameter)| test(parameter))
                .fold(0u64, |bits, (index, _)| bits | 1 << index)
        };
        let typed = bits(|parameter| parameter.ty.is_some());
        let special = bits(|parameter| parameter.by_reference || parameter.variadic);
        let by_reference = bits(|parameter| parameter.by_reference && !parameter.variadic);
        let general = body.parameters.len() > 64
            || body.argument_layout == phera_bytecode::ArgumentLayout::Variadic;
        Self {
            instructions: body.instructions.clone(),
            unit,
            local,
            unit_first,
            registers: body.registers,
            parameters: body.parameters.len() as u16,
            required: body
                .parameters
                .iter()
                .rposition(|parameter| !parameter.has_default && !parameter.variadic)
                .map_or(0, |index| index as u16 + 1),
            typed,
            special,
            by_reference,
            parameter_masks: masks,
            return_mask: body.return_type.map(mask_of),
            shape: if general {
                FunctionShape::General
            } else {
                FunctionShape::Plain
            },
            class: None,
        }
    }
}
