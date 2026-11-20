//! `function f(int $n, &$result): bool {}` has the same call facts in
//! every request, even though each request gives the function its own index.

use std::sync::Arc;

use crate::{ArgumentLayout, CompiledUnit, Function, Parameter, Type, TypeMask};

/// Unit-local lookup facts with no request-dependent indexes or PHP values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitMetadata {
    pub functions: Box<[FunctionSignature]>,
    pub names: Box<[String]>,
    pub symbols: Box<[String]>,
    pub type_scopes: Arc<[DeclarationScope]>,
}

impl UnitMetadata {
    pub fn new(unit: &CompiledUnit) -> Self {
        Self {
            functions: std::iter::once(&unit.main)
                .chain(&unit.functions)
                .map(|body| FunctionSignature::new(body, &unit.types))
                .collect(),
            names: unit
                .names
                .iter()
                .map(|name| name.to_ascii_lowercase())
                .collect(),
            symbols: unit
                .symbols
                .iter()
                .map(|symbol| symbol.name.to_ascii_lowercase())
                .collect(),
            type_scopes: unit.types.iter().map(DeclarationScope::of).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    pub registers: u16,
    pub parameters: u16,
    pub required: u16,
    pub typed: u64,
    pub special: u64,
    pub by_reference: u64,
    pub parameter_masks: Arc<[TypeMask]>,
    pub return_mask: Option<TypeMask>,
    pub shape: FunctionShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionShape {
    /// At most 64 parameters, none variadic.
    Plain,
    /// The binder handles parameters outside the bit masks and variadics.
    General,
}

impl FunctionSignature {
    pub fn new(body: &Function, types: &[Type]) -> Self {
        let mask_of = |ty: crate::TypeId| types[ty.index()].mask;
        let mut typed = 0;
        let mut special = 0;
        let mut by_reference = 0;
        for (index, parameter) in body.parameters.iter().take(64).enumerate() {
            let bit = 1u64 << index;
            typed |= u64::from(parameter.ty.is_some()) * bit;
            special |= u64::from(parameter.by_reference || parameter.variadic) * bit;
            by_reference |= u64::from(parameter.by_reference && !parameter.variadic) * bit;
        }
        let general =
            body.parameters.len() > 64 || body.argument_layout == ArgumentLayout::Variadic;
        Self {
            registers: body.registers,
            parameters: body.parameters.len() as u16,
            required: body
                .parameters
                .iter()
                .rposition(required)
                .map_or(0, |i| i as u16 + 1),
            typed,
            special,
            by_reference,
            parameter_masks: body
                .parameters
                .iter()
                .map(|parameter| parameter.ty.map_or(TypeMask::MIXED, mask_of))
                .collect(),
            return_mask: body.return_type.map(mask_of),
            shape: if general {
                FunctionShape::General
            } else {
                FunctionShape::Plain
            },
        }
    }
}

fn required(parameter: &Parameter) -> bool {
    !parameter.has_default && !parameter.variadic
}

/// A type mentioning a relative class keyword is bound in the callee's scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationScope {
    Absolute,
    Relative,
}

impl DeclarationScope {
    pub fn of(declaration: &Type) -> Self {
        if declaration
            .display
            .split(['|', '&', '?', '(', ')'])
            .any(|name| matches!(name, "self" | "parent" | "static"))
        {
            Self::Relative
        } else {
            Self::Absolute
        }
    }
}
