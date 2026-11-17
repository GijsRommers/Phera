//! Every function a request can run, in one flat table addressed by number.
//!
//! When a file is loaded, its top-level code and each function it declares
//! get a [`FunctionIndex`]: consecutive numbers, top-level code first.
//!
//! ```php
//! <?php                      // a.php: {main} is function 0
//! function f($x) { ... }     //        f is function 1
//! function g() { ... }       //        g is function 2
//! include 'b.php';           // b.php: {main} is function 3, and so on
//! ```
//!
//! A frame, a declared function and a method all refer to their code by this
//! number. Nothing is reference-counted to make a call: the table owns the
//! compiled units for the whole request, and the facts a call needs on every
//! entry (how many registers, how many parameters, which are typed) sit in a
//! small [`FunctionFacts`] record next to the number.

use std::sync::Arc;

use phera_bytecode::{CompiledUnit, Function, Instruction, Type, TypeMask};

use crate::unit_caches::UnitCache;

/// A function's position in the request's code table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FunctionIndex(pub(crate) u32);

/// A name, lowercased, as one number for the whole request: every file that
/// writes `->add(` gets the same key for `add`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NameKey(pub(crate) u32);

/// A loaded file's position in the request's code table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UnitIndex(u32);

impl FunctionIndex {
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

/// One loaded file: its bytecode and the request-local values made from it.
pub(crate) struct LoadedUnit {
    pub(crate) unit: Arc<CompiledUnit>,
    pub(crate) cache: UnitCache,
    /// The number of the file's top-level code; its functions follow it.
    first: FunctionIndex,
    /// The key of each of the file's names, lowercased.
    name_keys: Box<[NameKey]>,
    /// The key of each of the file's symbols (functions, classes, methods
    /// referred to by name), lowercased.
    symbol_keys: Box<[NameKey]>,
}

/// What a call needs to know about the function it enters, read on every
/// call, so it is kept small and next to nothing else.
pub(crate) struct FunctionFacts {
    unit: UnitIndex,
    /// `None` for a file's top-level code, else its number within the file.
    local: Option<u32>,
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
    /// The declared built-in types of each parameter, for the one-bit check.
    pub(crate) parameter_masks: Box<[TypeMask]>,
    /// The declared return type's built-in types, when there is a return type.
    pub(crate) return_mask: Option<TypeMask>,
    pub(crate) shape: FunctionShape,
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
    /// Whether parameter `index` has a declared type.
    pub(crate) fn is_typed(&self, index: usize) -> bool {
        index < 64 && self.typed & (1 << index) != 0
    }

    /// Whether parameter `index` is declared and takes a copy of its
    /// argument: it is neither by reference nor variadic.
    pub(crate) fn takes_value(&self, index: usize) -> bool {
        index < self.parameters as usize && index < 64 && self.special & (1 << index) == 0
    }

    fn new(
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
        let general = body.parameters.len() > 64
            || body.argument_layout == phera_bytecode::ArgumentLayout::Variadic;
        Self {
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
            parameter_masks: masks,
            return_mask: body.return_type.map(mask_of),
            shape: if general {
                FunctionShape::General
            } else {
                FunctionShape::Plain
            },
        }
    }
}

/// The code of one request: every loaded file and every function in them.
#[derive(Default)]
pub(crate) struct CodeTable {
    units: Vec<LoadedUnit>,
    functions: Vec<FunctionFacts>,
    /// Every lowercased name of every loaded file, numbered.
    names: std::collections::HashMap<String, NameKey>,
}

impl CodeTable {
    /// Adds a compiled file and numbers its functions, or finds it when it
    /// was loaded before. Returns the number of its top-level code.
    pub(crate) fn load(&mut self, unit: &Arc<CompiledUnit>) -> FunctionIndex {
        if let Some(loaded) = self
            .units
            .iter()
            .find(|loaded| Arc::ptr_eq(&loaded.unit, unit))
        {
            return loaded.first;
        }
        let index = UnitIndex(self.units.len() as u32);
        let first = FunctionIndex(self.functions.len() as u32);
        let bodies = std::iter::once((None, &unit.main)).chain(
            unit.functions
                .iter()
                .enumerate()
                .map(|(local, body)| (Some(local as u32), body)),
        );
        for (local, body) in bodies {
            let facts = FunctionFacts::new((index, first, local), body, &unit.types);
            self.functions.push(facts);
        }
        let name_keys = unit
            .names
            .iter()
            .map(|name| self.intern(name.to_ascii_lowercase()))
            .collect();
        let symbol_keys = unit
            .symbols
            .iter()
            .map(|symbol| self.intern(symbol.name.to_ascii_lowercase()))
            .collect();
        self.units.push(LoadedUnit {
            unit: unit.clone(),
            cache: UnitCache::new(unit),
            first,
            name_keys,
            symbol_keys,
        });
        first
    }

    /// The key of a lowercased name, numbering it if it is new.
    fn intern(&mut self, lowercase: String) -> NameKey {
        let next = NameKey(self.names.len() as u32);
        *self.names.entry(lowercase).or_insert(next)
    }

    /// The key of a lowercased name, if some loaded file uses the name.
    pub(crate) fn name_key(&self, lowercase: &str) -> Option<NameKey> {
        self.names.get(lowercase).copied()
    }

    /// The key of symbol `symbol` of the file `function` belongs to.
    #[inline]
    pub(crate) fn unit_symbol_key(
        &self,
        function: FunctionIndex,
        symbol: phera_bytecode::SymbolId,
    ) -> NameKey {
        self.loaded(function).symbol_keys[symbol.index()]
    }

    /// The key of name `name` of the file `function` belongs to.
    #[inline]
    pub(crate) fn unit_name_key(
        &self,
        function: FunctionIndex,
        name: phera_bytecode::NameId,
    ) -> NameKey {
        self.loaded(function).name_keys[name.index()]
    }

    /// The number of function `local` of the file whose top-level code is `first`.
    pub(crate) fn declared(&self, first: FunctionIndex, local: usize) -> FunctionIndex {
        FunctionIndex(first.0 + 1 + local as u32)
    }

    /// The number of a body of an already loaded unit; `None` is top-level code.
    pub(crate) fn index_in(&self, unit: &Arc<CompiledUnit>, local: Option<usize>) -> FunctionIndex {
        let first = self
            .units
            .iter()
            .find(|loaded| Arc::ptr_eq(&loaded.unit, unit))
            .map_or(FunctionIndex(0), |loaded| loaded.first);
        match local {
            Some(local) => self.declared(first, local),
            None => first,
        }
    }

    /// Function `local` of the file whose top-level code is `first`, with its
    /// facts; `None` if the file has no such function.
    #[inline]
    pub(crate) fn declared_facts(
        &self,
        first: FunctionIndex,
        local: usize,
    ) -> Option<(FunctionIndex, &FunctionFacts)> {
        let function = self.declared(first, local);
        let facts = self.functions.get(function.index())?;
        (facts.unit_first == first).then_some((function, facts))
    }

    #[inline]
    pub(crate) fn facts(&self, function: FunctionIndex) -> &FunctionFacts {
        &self.functions[function.index()]
    }

    fn loaded(&self, function: FunctionIndex) -> &LoadedUnit {
        &self.units[self.facts(function).unit.0 as usize]
    }

    /// The compiled file a function belongs to.
    pub(crate) fn unit(&self, function: FunctionIndex) -> &Arc<CompiledUnit> {
        &self.loaded(function).unit
    }

    /// The request-local values made from that file (names, literals, sites).
    pub(crate) fn cache(&self, function: FunctionIndex) -> &UnitCache {
        &self.loaded(function).cache
    }

    /// The function's number within its file; `None` for top-level code.
    pub(crate) fn local(&self, function: FunctionIndex) -> Option<usize> {
        self.facts(function).local.map(|local| local as usize)
    }

    /// The function's bytecode.
    pub(crate) fn function(&self, function: FunctionIndex) -> &Function {
        body(self.unit(function), self.local(function))
    }

    /// The function's instructions, which the interpreter loop borrows.
    #[inline]
    pub(crate) fn instructions(&self, function: FunctionIndex) -> &[Instruction] {
        &self.function(function).instructions
    }
}

/// Function `local` of a file, or its top-level code for `None`.
pub(crate) fn body(unit: &CompiledUnit, local: Option<usize>) -> &Function {
    match local {
        Some(local) => &unit.functions[local],
        None => &unit.main,
    }
}

#[cfg(test)]
mod tests {
    use super::CodeTable;
    use std::sync::Arc;

    /// Loaded units stay alive for the request after every frame has returned.
    #[test]
    fn cached_units_stay_alive() {
        let unit = Arc::new(phera_compiler::compile("<?php", "cache.php").unwrap());
        let weak = Arc::downgrade(&unit);
        let mut code = CodeTable::default();
        code.load(&unit);
        drop(unit);
        assert!(weak.upgrade().is_some());
    }
}
