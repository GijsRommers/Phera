//! `function f(int $n, &$result): bool {}` has the same call facts in
//! every request, even though each request gives the function its own index.

use std::sync::Arc;

use crate::{ArgumentLayout, CompiledUnit, Function, Parameter, Type, TypeMask};

/// Unit-local lookup facts with no request-dependent indexes or PHP values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitMetadata {
    /// The file revision's number in the process, from 1: a request finds
    /// a file it loaded already at this position of a flat table.
    pub id: u32,
    pub functions: Box<[FunctionSignature]>,
    pub names: Box<[String]>,
    pub symbols: Box<[String]>,
    /// The process's key of each name and of each symbol (see
    /// [`crate::name_keys`]), given when the file was compiled.
    pub name_keys: Arc<[crate::NameKey]>,
    pub symbol_keys: Arc<[crate::NameKey]>,
    /// The key of each class the file declares, by its lowercased name: a
    /// declaration looks itself up by it.
    pub class_keys: Arc<[crate::NameKey]>,
    pub type_scopes: Arc<[DeclarationScope]>,
}

impl UnitMetadata {
    pub fn new(unit: &CompiledUnit) -> Self {
        let names: Box<[String]> = unit
            .names
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .collect();
        let symbols: Box<[String]> = unit
            .symbols
            .iter()
            .map(|symbol| symbol.name.to_ascii_lowercase())
            .collect();
        // Four billion compiled file revisions would not fit in memory first.
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        Self {
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            name_keys: crate::name_keys::keys(names.iter().map(String::as_str)),
            symbol_keys: crate::name_keys::keys(symbols.iter().map(String::as_str)),
            class_keys: class_keys(unit),
            functions: std::iter::once(&unit.main)
                .chain(&unit.functions)
                .map(|body| FunctionSignature::new(body, &unit.types))
                .collect(),
            names,
            symbols,
            type_scopes: unit.types.iter().map(DeclarationScope::of).collect(),
        }
    }
}

/// The key of each class `unit` declares, by its lowercased name.
pub fn class_keys(unit: &CompiledUnit) -> Arc<[crate::NameKey]> {
    let names: Vec<String> = unit
        .classes
        .iter()
        .map(|class| class.name.to_ascii_lowercase())
        .collect();
    crate::name_keys::keys(names.iter().map(String::as_str))
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
    /// A return from this body needs neither type checking nor alias binding.
    pub untyped_value_return: bool,
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
            untyped_value_return: body.return_type.is_none()
                && !body.returns_reference
                && !body.is_generator,
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
