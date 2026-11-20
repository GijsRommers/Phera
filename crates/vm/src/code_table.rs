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

use phera_bytecode::{CompiledUnit, Function, FunctionSignature, Instruction};

pub(crate) use crate::function_facts::{FunctionFacts, FunctionShape};

use crate::unit_caches::UnitCache;

/// A function's position in the request's code table.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) struct FunctionIndex(pub(crate) u32);

/// A name, lowercased, as one number for the whole request: every file that
/// writes `->add(` gets the same key for `add`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NameKey(pub(crate) u32);

/// A loaded file's position in the request's code table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UnitIndex(pub(crate) u32);

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

/// The code of one request: every loaded file and every function in them.
#[derive(Default)]
pub(crate) struct CodeTable {
    units: Vec<LoadedUnit>,
    functions: Vec<FunctionFacts>,
    /// Every lowercased name of every loaded file, numbered.
    names: std::collections::HashMap<String, NameKey>,
    trait_method_names: std::collections::HashMap<FunctionIndex, String>,
}

impl CodeTable {
    pub(crate) fn next_unit_index(&self) -> UnitIndex {
        UnitIndex(self.units.len() as u32)
    }

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
        let index = self.next_unit_index();
        let first = FunctionIndex(self.functions.len() as u32);
        let (name_keys, symbol_keys) = if let Some(metadata) = &unit.metadata {
            self.functions
                .extend(metadata.functions.iter().cloned().enumerate().map(
                    |(offset, signature)| {
                        let local = offset.checked_sub(1).map(|local| local as u32);
                        let instructions = body(unit, local.map(|local| local as usize))
                            .instructions
                            .clone();
                        FunctionFacts::bind((index, first, local), signature, instructions)
                    },
                ));
            let names = metadata
                .names
                .iter()
                .map(|name| self.intern_shared(name))
                .collect();
            let symbols = metadata
                .symbols
                .iter()
                .map(|name| self.intern_shared(name))
                .collect();
            (names, symbols)
        } else {
            let bodies = std::iter::once(&unit.main).chain(&unit.functions);
            self.functions
                .extend(bodies.enumerate().map(|(offset, body)| {
                    let local = offset.checked_sub(1).map(|local| local as u32);
                    FunctionFacts::bind(
                        (index, first, local),
                        FunctionSignature::new(body, &unit.types),
                        body.instructions.clone(),
                    )
                }));
            let names = unit
                .names
                .iter()
                .map(|name| self.intern(name.to_ascii_lowercase()))
                .collect();
            let symbols = unit
                .symbols
                .iter()
                .map(|symbol| self.intern(symbol.name.to_ascii_lowercase()))
                .collect();
            (names, symbols)
        };
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

    fn intern_shared(&mut self, lowercase: &str) -> NameKey {
        match self.names.get(lowercase) {
            Some(key) => *key,
            None => self.intern(lowercase.to_owned()),
        }
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

    /// Record that `function` is a method of `class`, once the class is linked.
    pub(crate) fn set_declaring_class(
        &mut self,
        function: FunctionIndex,
        class: crate::classes::ClassIndex,
    ) {
        self.functions[function.index()].class = Some(class);
    }

    /// Flattened methods share immutable instructions and own their scope and static cells.
    fn import_trait_body(
        &mut self,
        source: FunctionIndex,
        scope: crate::classes::ClassIndex,
    ) -> FunctionIndex {
        let mut facts = self.facts(source).clone();
        facts.class = Some(scope);
        let function = FunctionIndex(self.functions.len() as u32);
        self.functions.push(facts);
        function
    }

    pub(crate) fn import_trait_method(
        &mut self,
        source: FunctionIndex,
        scope: crate::classes::ClassIndex,
        name: String,
    ) -> FunctionIndex {
        let function = self.import_trait_body(source, scope);
        self.trait_method_names.insert(function, name);
        function
    }

    pub(crate) fn function_name(&self, function: FunctionIndex) -> &str {
        self.trait_method_names
            .get(&function)
            .map_or_else(|| self.function(function).name.as_str(), String::as_str)
    }

    /// Composed methods reached through multiple traits can share the original body.
    pub(crate) fn original_function(&self, source: FunctionIndex) -> FunctionIndex {
        let facts = self.facts(source);
        facts.local.map_or(facts.unit_first, |local| {
            self.declared(facts.unit_first, local as usize)
        })
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

    pub(crate) fn known_unit(&self, function: FunctionIndex) -> Option<&Arc<CompiledUnit>> {
        let facts = self.functions.get(function.index())?;
        self.units.get(facts.unit.0 as usize).map(|unit| &unit.unit)
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
    /// They are read from the function's facts, not through its file, on
    /// every call and return.
    #[inline]
    pub(crate) fn instructions(&self, function: FunctionIndex) -> &[Instruction] {
        &self.facts(function).instructions
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
