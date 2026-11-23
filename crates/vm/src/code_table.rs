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

use crate::unit_caches::{CachedSite, Sites, UnitCache};

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
    /// The case-sensitive property key of each of the file's names.
    prop_keys: Arc<[phera_bytecode::PropKey]>,
    /// The key of each of the file's symbols (functions, classes, methods
    /// referred to by name), lowercased.
    symbol_keys: Arc<[NameKey]>,
    /// The key of each class the file declares, lowercased.
    class_keys: Arc<[NameKey]>,
}

/// The code of one request: every loaded file and every function in them.
pub(crate) struct CodeTable {
    units: Vec<LoadedUnit>,
    /// The facts of every function by its number, which the worker gives
    /// (see `crate::worker_code`). It covers every number up to the highest
    /// this request uses, so it also holds placeholders for functions of
    /// files this request has not loaded: whether a function is declared in
    /// the request is its function table's answer, never this vector's.
    ///
    /// A fork has its own copy, a vector read as it always was: sharing it
    /// with the snapshot (an `Rc<[FunctionFacts]>`) made every read of the
    /// facts dearer than a `Vec`'s, by more than the copy costs a fork (see
    /// the decision file of 2026-11-22).
    functions: Vec<FunctionFacts>,
    worker: std::rc::Rc<crate::worker_code::WorkerCode>,
    /// The numbers this request leased, and those of the snapshot it was
    /// forked from, which it keeps alive.
    leases: Vec<std::rc::Rc<crate::worker_code::Leases>>,
    /// Where each loaded file is, by its process number
    /// (`UnitMetadata::id`): its position in `units` plus one, 0 if this
    /// request has not loaded it.
    slots: Vec<u32>,
    /// The site cells of every function by its number (top-level code
    /// included), made when the function first looks a site up: one cell
    /// per instruction. A fork's start empty, with `base_sites` to make them
    /// from. Grows with `functions`.
    sites: Vec<Sites>,
    /// What the snapshot's functions had filled: shared by its forks, never
    /// changed.
    base_sites: Option<std::rc::Rc<[Sites]>>,
    /// The name of each trait method composed into a class, which can differ
    /// from the name of the body it runs (`use T { foo as bar }`), by the
    /// method's number, sorted. Read only to name a function in an error or
    /// a trace, so it is a sorted array searched by number, held like the
    /// facts by a snapshot and its forks.
    trait_method_names: std::rc::Rc<Vec<(u32, std::rc::Rc<str>)>>,
    /// While a class links: the trait methods composed into it.
    imports: Option<Vec<crate::classes::worker_classes::Import>>,
}

impl Default for CodeTable {
    /// A table with a numbering of its own (a command-line run, a test).
    fn default() -> Self {
        Self::for_worker(&std::rc::Rc::default())
    }
}

impl CodeTable {
    pub(crate) fn loaded_units(&self) -> impl Iterator<Item = &CompiledUnit> {
        self.units.iter().map(|loaded| loaded.unit.as_ref())
    }

    /// A request's table, numbered by its worker.
    pub(crate) fn for_worker(worker: &std::rc::Rc<crate::worker_code::WorkerCode>) -> Self {
        CodeTable {
            units: Vec::new(),
            functions: Vec::new(),
            worker: worker.clone(),
            leases: vec![crate::worker_code::Leases::new(worker)],
            slots: Vec::new(),
            sites: Vec::new(),
            base_sites: None,
            trait_method_names: Default::default(),
            imports: None,
        }
    }

    /// Starts recording the trait methods composed into a class being linked.
    pub(crate) fn begin_imports(&mut self) {
        self.imports = Some(Vec::new());
    }

    /// The trait methods composed since [`CodeTable::begin_imports`].
    pub(crate) fn end_imports(&mut self) -> Vec<crate::classes::worker_classes::Import> {
        self.imports.take().unwrap_or_default()
    }

    /// Keeps the number of a composed trait method for every request of the
    /// worker, instead of giving it back with this request.
    pub(crate) fn keep_import(&self, number: u32) {
        self.own_leases().keep(number);
    }

    /// Writes the facts of a trait method another request composed into a
    /// class this request now declares.
    pub(crate) fn write_import(
        &mut self,
        (number, source, name): &crate::classes::worker_classes::Import,
        scope: crate::classes::ClassIndex,
    ) {
        let mut facts = self.facts(FunctionIndex(*source)).clone();
        facts.class = Some(scope);
        let index = *number as usize;
        self.cover(index + 1);
        self.functions[index] = facts;
        self.name_trait_method(*number, name);
    }

    /// Records the name of the trait method `number`, unless it has it.
    fn name_trait_method(&mut self, number: u32, name: &str) {
        let position = self
            .trait_method_names
            .binary_search_by_key(&number, |(method, _)| *method);
        match position {
            Ok(found) if &*self.trait_method_names[found].1 == name => {}
            Ok(found) => std::rc::Rc::make_mut(&mut self.trait_method_names)[found].1 = name.into(),
            Err(at) => std::rc::Rc::make_mut(&mut self.trait_method_names)
                .insert(at, (number, name.into())),
        }
    }

    /// The worker's numbering this table uses.
    pub(crate) fn worker(&self) -> std::rc::Rc<crate::worker_code::WorkerCode> {
        self.worker.clone()
    }

    /// The leases of this request, the last of `leases`.
    fn own_leases(&self) -> &crate::worker_code::Leases {
        self.leases
            .last()
            .expect("a code table always has its own leases")
    }

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
        let count = 1 + unit.functions.len() as u32;
        let first = FunctionIndex(self.worker.number(unit, count, self.own_leases()));
        let mut facts = Vec::with_capacity(count as usize);
        let (name_keys, prop_keys, symbol_keys, class_keys) = if let Some(metadata) = &unit.metadata
        {
            facts.extend(metadata.functions.iter().cloned().enumerate().map(
                |(offset, signature)| {
                    let local = offset.checked_sub(1).map(|local| local as u32);
                    let instructions = body(unit, local.map(|local| local as usize))
                        .instructions
                        .clone();
                    FunctionFacts::bind((index, first, local), signature, instructions)
                },
            ));
            (
                metadata.name_keys.clone(),
                metadata.prop_keys.clone(),
                metadata.symbol_keys.clone(),
                metadata.class_keys.clone(),
            )
        } else {
            let bodies = std::iter::once(&unit.main).chain(&unit.functions);
            facts.extend(bodies.enumerate().map(|(offset, body)| {
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
                phera_bytecode::name_keys::prop_keys(unit.names.iter().map(String::as_str)),
                phera_bytecode::name_keys::keys(symbols.iter().map(String::as_str)),
                phera_bytecode::class_keys(unit),
            )
        };
        let start = first.index();
        self.cover(start + facts.len());
        for (slot, facts) in self.functions[start..].iter_mut().zip(facts) {
            *slot = facts;
        }
        self.units.push(LoadedUnit {
            unit: unit.clone(),
            cache: UnitCache::new(unit),
            first,
            name_keys,
            prop_keys,
            symbol_keys,
            class_keys,
        });
        first
    }

    /// Makes the class checks of the file `function` belongs to, if a fork has
    /// not yet (see [`crate::unit_caches::ClassTypes`]): the slow path of a
    /// typed parameter or return value does it before it looks a check up.
    pub(crate) fn make_class_types(&mut self, function: FunctionIndex) {
        let unit = self.facts(function).unit;
        if let Some(loaded) = self.units.get_mut(unit.0 as usize) {
            loaded.cache.class_types.make();
        }
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

    /// The key of the name of class `class` of the file `function` belongs to.
    #[inline]
    pub(crate) fn unit_class_key(
        &self,
        function: FunctionIndex,
        class: phera_bytecode::ClassId,
    ) -> NameKey {
        self.loaded(function).class_keys[class.index()]
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

    /// The case-sensitive property key of name `name` of the file `function` belongs to.
    #[inline]
    pub(crate) fn unit_prop_key(
        &self,
        function: FunctionIndex,
        name: phera_bytecode::NameId,
    ) -> phera_bytecode::PropKey {
        self.loaded(function).prop_keys[name.index()]
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
        let function = FunctionIndex(self.worker.lease_one(self.own_leases()));
        self.cover(function.index() + 1);
        self.functions[function.index()] = facts;
        function
    }

    pub(crate) fn import_trait_method(
        &mut self,
        source: FunctionIndex,
        scope: crate::classes::ClassIndex,
        name: String,
    ) -> FunctionIndex {
        let function = self.import_trait_body(source, scope);
        if let Some(imports) = &mut self.imports {
            imports.push((function.0, source.0, name.clone()));
        }
        self.name_trait_method(function.0, &name);
        function
    }

    pub(crate) fn function_name(&self, function: FunctionIndex) -> &str {
        match self
            .trait_method_names
            .binary_search_by_key(&function.0, |(method, _)| *method)
        {
            Ok(found) => &self.trait_method_names[found].1,
            Err(_) => self.function(function).name.as_str(),
        }
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

    /// Makes room for numbers up to `end`, with placeholders, and for their
    /// site cells.
    fn cover(&mut self, end: usize) {
        if self.functions.len() < end {
            self.functions.resize(end, FunctionFacts::placeholder());
            self.sites.resize_with(end, Sites::new);
        }
    }

    /// The site of the instruction of `function` that ends at `ip`: the VM
    /// looks it up after the instruction pointer has advanced. The cells of
    /// the function are made here, the first time any of its sites is
    /// looked up.
    #[inline]
    pub(crate) fn site(
        &self,
        function: FunctionIndex,
        ip: usize,
    ) -> Option<&std::cell::RefCell<CachedSite>> {
        self.sites
            .get(function.index())?
            .get_or_init(|| self.make_sites(function))
            .get(ip)
    }

    /// The cells of a function that first looks a site up: those the
    /// snapshot filled that a request may use, else empty ones.
    #[cold]
    #[inline(never)]
    fn make_sites(&self, function: FunctionIndex) -> Box<[std::cell::RefCell<CachedSite>]> {
        let instructions = self.facts(function).instructions.len();
        let known = self
            .base_sites
            .as_ref()
            .and_then(|base| base.get(function.index()))
            .and_then(|sites| sites.get());
        match known {
            Some(cells) if cells.len() == instructions + 1 => cells
                .iter()
                .map(|cell| std::cell::RefCell::new(cell.borrow().shared()))
                .collect(),
            _ => crate::unit_caches::empty_sites(instructions),
        }
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
    #[inline]
    pub(crate) fn cache(&self, function: FunctionIndex) -> &UnitCache {
        &self.loaded(function).cache
    }

    /// The function's number within its file; `None` for top-level code.
    pub(crate) fn local(&self, function: FunctionIndex) -> Option<usize> {
        self.facts(function).local.map(|local| local as usize)
    }

    /// The function's bytecode, its file's request-local cache and its
    /// facts, from one lookup of the facts.
    #[inline]
    pub(crate) fn function_parts(
        &self,
        function: FunctionIndex,
    ) -> (&Function, &UnitCache, &FunctionFacts) {
        let facts = self.facts(function);
        let loaded = &self.units[facts.unit.0 as usize];
        let local = facts.local.map(|local| local as usize);
        (body(&loaded.unit, local), &loaded.cache, facts)
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

impl CodeTable {
    /// What a fork's copy of this table holds on the heap, by structure, in
    /// bytes (see `crate::snapshot::footprint`).
    #[cfg(debug_assertions)]
    pub(crate) fn fork_footprint(&self) -> Vec<(&'static str, usize)> {
        use std::mem::size_of;
        let mut sums = [0; 3];
        for loaded in &self.units {
            for (sum, bytes) in sums.iter_mut().zip(loaded.cache.fork_bytes()) {
                *sum += bytes;
            }
        }
        let [types, functions, closures] = sums;
        let cells = self
            .sites
            .iter()
            .map(|sites| sites.get().map_or(0, |cells| cells.len()))
            .sum::<usize>()
            * size_of::<std::cell::RefCell<CachedSite>>();
        let boxes = self.sites.capacity() * size_of::<Sites>();
        vec![
            ("site cells", cells),
            ("per-function site boxes", boxes),
            ("per-type class checks", types),
            ("per-symbol function links", functions),
            ("closure signatures", closures),
            (
                "function facts",
                self.functions.capacity() * size_of::<FunctionFacts>(),
            ),
            (
                "loaded-file records",
                self.units.capacity() * size_of::<LoadedUnit>()
                    + self.slots.capacity() * size_of::<u32>(),
            ),
        ]
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
                    prop_keys: loaded.prop_keys.clone(),
                    symbol_keys: loaded.symbol_keys.clone(),
                    class_keys: loaded.class_keys.clone(),
                })
                .collect(),
            functions: self.functions.clone(),
            worker: self.worker.clone(),
            // The snapshot's leases stay given out while this fork lives.
            leases: self
                .leases
                .iter()
                .cloned()
                .chain([crate::worker_code::Leases::new(&self.worker)])
                .collect(),
            slots: self.slots.clone(),
            sites: (0..self.sites.len()).map(|_| Sites::new()).collect(),
            base_sites: match &self.base_sites {
                Some(base) => Some(base.clone()),
                None => Some(
                    self.sites
                        .iter()
                        .map(crate::unit_caches::frozen_sites)
                        .collect(),
                ),
            },
            trait_method_names: self.trait_method_names.clone(),
            imports: None,
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
