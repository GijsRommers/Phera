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

/// A name, lowercased, as one number for the whole process: every file that
/// writes `->add(` gets the same key for `add` (see `phera_bytecode::name_keys`).
pub(crate) use phera_bytecode::NameKey;

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
    /// The key of each of the file's names, lowercased; shared with the
    /// compiled file, which numbered them once.
    name_keys: Arc<[NameKey]>,
    /// The key of each of the file's symbols (functions, classes, methods
    /// referred to by name), lowercased.
    symbol_keys: Arc<[NameKey]>,
}

/// The code of one request: every loaded file and every function in them.
#[derive(Default)]
pub(crate) struct CodeTable {
    units: Vec<LoadedUnit>,
    functions: Vec<FunctionFacts>,
    /// Where each loaded file is, by its process number
    /// (`UnitMetadata::id`): its position in `units` plus one, 0 if this
    /// request has not loaded it.
    slots: Vec<u32>,
    trait_method_names: std::collections::HashMap<FunctionIndex, String>,
}

impl CodeTable {
    pub(crate) fn next_unit_index(&self) -> UnitIndex {
        UnitIndex(self.units.len() as u32)
    }

    /// Adds a compiled file and numbers its functions, or finds it when it
    /// was loaded before. Returns the number of its top-level code.
    pub(crate) fn load(&mut self, unit: &Arc<CompiledUnit>) -> FunctionIndex {
        if let Some(loaded) = self.find(unit) {
            return loaded.first;
        }
        let index = self.next_unit_index();
        if let Some(metadata) = &unit.metadata {
            let id = metadata.id as usize;
            if self.slots.len() <= id {
                self.slots.resize(id + 1, 0);
            }
            self.slots[id] = index.0 + 1;
        }
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
            (metadata.name_keys.clone(), metadata.symbol_keys.clone())
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
            let names: Vec<String> = unit
                .names
                .iter()
                .map(|name| name.to_ascii_lowercase())
                .collect();
            let symbols: Vec<String> = unit
                .symbols
                .iter()
                .map(|symbol| symbol.name.to_ascii_lowercase())
                .collect();
            (
                phera_bytecode::name_keys::keys(names.iter().map(String::as_str)),
                phera_bytecode::name_keys::keys(symbols.iter().map(String::as_str)),
            )
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

    /// The file, if this request loaded it: by its process number, or for a
    /// file compiled without metadata (command line, `eval`) by a search.
    fn find(&self, unit: &Arc<CompiledUnit>) -> Option<&LoadedUnit> {
        match &unit.metadata {
            Some(metadata) => {
                let slot = *self.slots.get(metadata.id as usize)?;
                let loaded = self.units.get(slot.checked_sub(1)? as usize)?;
                Arc::ptr_eq(&loaded.unit, unit).then_some(loaded)
            }
            None => self
                .units
                .iter()
                .find(|loaded| Arc::ptr_eq(&loaded.unit, unit)),
        }
    }

    /// The number of a body of an already loaded unit; `None` is top-level code.
    pub(crate) fn index_in(&self, unit: &Arc<CompiledUnit>, local: Option<usize>) -> FunctionIndex {
        let first = self
            .find(unit)
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

/// A fork has the snapshot's files with the same numbers, and its own
/// copies of their caches (see `crate::snapshot`).
impl crate::snapshot::ForkCopy for CodeTable {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        CodeTable {
            units: self
                .units
                .iter()
                .map(|loaded| LoadedUnit {
                    unit: loaded.unit.clone(),
                    cache: loaded.cache.fork_copy(fork),
                    first: loaded.first,
                    name_keys: loaded.name_keys.clone(),
                    symbol_keys: loaded.symbol_keys.clone(),
                })
                .collect(),
            functions: self.functions.clone(),
            slots: self.slots.clone(),
            trait_method_names: self.trait_method_names.clone(),
        }
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
