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
    /// Per-request site cells, allocated lazily only when a request writes a
    /// site not already satisfied by `base_sites`. Empty on warmed forks.
    sites: std::cell::RefCell<Vec<Option<Sites>>>,
    /// Fast guard so reads on warmed forks skip `sites.borrow()` completely
    /// and read directly from immutable `base_sites`.
    has_local_sites: std::cell::Cell<bool>,
    /// What the snapshot's functions had filled: immutable, shared by every
    /// fork via `Rc::clone`, read directly with zero `RefCell` borrows.
    base_sites: Option<std::rc::Rc<[Option<Sites>]>>,
    /// The name of each trait method composed into a class, which can differ
    /// from the name of the body it runs (`use T { foo as bar }`), by the
    /// method's number, sorted. Read only to name a function in an error or
    /// a trace, so it is a sorted array searched by number, held like the
    /// facts by a snapshot and its forks.
    trait_method_names: std::rc::Rc<Vec<(u32, std::rc::Rc<str>)>>,
    /// While a class links: the trait methods composed into it.
    imports: Option<Vec<crate::classes::worker_classes::Import>>,
    /// Lazily lowered Tier-2 compact functions indexed by `FunctionIndex`.
    fast_tier: std::cell::RefCell<Vec<crate::fast_tier::FastEntry>>,
    /// Guard so HTTP forks/rollbacks never touch `fast_tier.borrow()`.
    has_fast_tier: std::cell::Cell<bool>,
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
            sites: std::cell::RefCell::new(Vec::new()),
            has_local_sites: std::cell::Cell::new(false),
            base_sites: None,
            trait_method_names: Default::default(),
            imports: None,
            fast_tier: std::cell::RefCell::new(Vec::new()),
            has_fast_tier: std::cell::Cell::new(false),
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

    pub(crate) fn units_len(&self) -> usize {
        self.units.len()
    }

    pub(crate) fn functions_len(&self) -> usize {
        self.functions.len()
    }

    pub(crate) fn trait_method_names(&self) -> std::rc::Rc<Vec<(u32, std::rc::Rc<str>)>> {
        self.trait_method_names.clone()
    }

    /// Discards any request-local site overrides before returning this table
    /// to a snapshot's recycle pool.
    pub(crate) fn clear_local_sites(&self) {
        if self.has_local_sites.get() {
            self.sites.borrow_mut().clear();
            self.has_local_sites.set(false);
        }
    }

    /// Rolls back request-local units, function facts, leases, and dirty caches
    /// to the originating snapshot's boundary so the table can be recycled.
    pub(crate) fn rollback_to_snapshot(
        &mut self,
        units_len: usize,
        functions_len: usize,
        trait_names: &std::rc::Rc<Vec<(u32, std::rc::Rc<str>)>>,
        clear_unit_caches: bool,
    ) {
        if self.units.len() > units_len {
            for loaded in self.units.drain(units_len..) {
                if let Some(metadata) = &loaded.unit.metadata {
                    let id = metadata.id as usize;
                    if let Some(slot) = self.slots.get_mut(id) {
                        *slot = 0;
                    }
                }
            }
            if let Some(own) = self.leases.last_mut() {
                *own = crate::worker_code::Leases::new(&self.worker);
            }
        }
        if self.functions.len() > functions_len {
            self.functions.truncate(functions_len);
        }
        if self.has_fast_tier.get() {
            let mut fast = self.fast_tier.borrow_mut();
            if fast.len() > functions_len {
                fast.truncate(functions_len);
            }
        }
        if self.trait_method_names.len() != trait_names.len() {
            self.trait_method_names = trait_names.clone();
        }
        self.clear_local_sites();
        if clear_unit_caches {
            for loaded in &mut self.units {
                loaded.cache.reset_mutable_cells();
            }
        }
    }

    /// Empty placeholder table with zero heap allocations, used when moving a
    /// clean `CodeTable` out of a dropping `Vm` back into a snapshot pool.
    pub(crate) fn detached(&self) -> Self {
        CodeTable {
            units: Vec::new(),
            functions: Vec::new(),
            worker: self.worker.clone(),
            leases: Vec::new(),
            slots: Vec::new(),
            sites: std::cell::RefCell::new(Vec::new()),
            has_local_sites: std::cell::Cell::new(false),
            base_sites: None,
            trait_method_names: Default::default(),
            imports: None,
            fast_tier: std::cell::RefCell::new(Vec::new()),
            has_fast_tier: std::cell::Cell::new(false),
        }
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
        let (first_num, fast_candidates) =
            self.worker.number_with_fast(unit, count, self.own_leases());
        let first = FunctionIndex(first_num);
        let mut facts = Vec::with_capacity(count as usize);
        let (name_keys, prop_keys, symbol_keys, class_keys) = if let Some(metadata) = &unit.metadata
        {
            let has_fast = !fast_candidates.is_none();
            facts.extend(metadata.functions.iter().cloned().enumerate().map(
                |(offset, signature)| {
                    let local = offset.checked_sub(1).map(|local| local as u32);
                    let mut entry = FunctionFacts::bind(
                        (index, first, local),
                        signature,
                        body(unit, local.map(|local| local as usize))
                            .instructions
                            .clone(),
                    );
                    if has_fast {
                        entry.fast_candidate = fast_candidates.get(offset);
                    }
                    entry
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
                let mut entry = FunctionFacts::bind(
                    (index, first, local),
                    FunctionSignature::new(body, &unit.types),
                    body.instructions.clone(),
                );
                entry.fast_candidate = crate::fast_tier::lower::is_fast_candidate(body, local);
                entry
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

    /// Makes room for numbers up to `end`, with placeholders.
    fn cover(&mut self, end: usize) {
        if self.functions.len() < end {
            self.functions.resize(end, FunctionFacts::placeholder());
        }
    }

    /// Reads the cached site for `function` at `ip`.
    ///
    /// On warmed forks (`!has_local_sites`), reads directly from immutable
    /// `base_sites` with zero `RefCell` borrows and zero allocations.
    #[inline(always)]
    pub(crate) fn read_site<R>(
        &self,
        function: FunctionIndex,
        ip: usize,
        visit: impl FnOnce(&CachedSite) -> R,
    ) -> Option<R> {
        if !self.has_local_sites.get() {
            let site = self
                .base_sites
                .as_ref()?
                .get(function.index())?
                .as_deref()?
                .get(ip)?;
            return Some(visit(site));
        }
        self.read_site_local(function, ip, visit)
    }

    #[inline]
    fn read_site_local<R>(
        &self,
        function: FunctionIndex,
        ip: usize,
        visit: impl FnOnce(&CachedSite) -> R,
    ) -> Option<R> {
        if let Ok(sites) = self.sites.try_borrow()
            && let Some(cells) = sites.get(function.index()).and_then(Option::as_deref)
        {
            return cells.get(ip).map(visit);
        }
        let site = self
            .base_sites
            .as_ref()?
            .get(function.index())?
            .as_deref()?
            .get(ip)?;
        Some(visit(site))
    }

    /// Stores `value` in the request-local site cell for `function` at `ip`,
    /// initializing that function's site slice lazily on its first write.
    #[cold]
    #[inline(never)]
    pub(crate) fn write_site(&self, function: FunctionIndex, ip: usize, value: CachedSite) {
        let index = function.index();
        if index >= self.functions.len() {
            return;
        }
        let mut sites = self.sites.borrow_mut();
        if sites.len() <= index {
            sites.resize_with(index + 1, || None);
        }
        let cells = sites[index].get_or_insert_with(|| self.make_sites(function));
        if let Some(cell) = cells.get_mut(ip) {
            *cell = value;
            self.has_local_sites.set(true);
        }
    }

    /// The cells of a function on its first local site write: those the
    /// snapshot filled that a request may use, else empty ones.
    #[cold]
    #[inline(never)]
    fn make_sites(&self, function: FunctionIndex) -> Sites {
        let instructions = self.facts(function).instructions.len();
        let known = self
            .base_sites
            .as_ref()
            .and_then(|base| base.get(function.index()))
            .and_then(Option::as_deref);
        match known {
            Some(cells) if cells.len() == instructions + 1 => {
                crate::unit_caches::frozen_sites(cells)
            }
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

    /// Borrow the Tier-2 compiled function table, lazily lowering `function`
    /// (and its direct callees) on first encounter. Returns `None` if `function`
    /// is ineligible for Tier 2.
    pub(crate) fn ensure_fast(
        &self,
        function: FunctionIndex,
    ) -> Option<std::cell::Ref<'_, [crate::fast_tier::FastEntry]>> {
        use crate::fast_tier::FastEntry;
        let idx = function.index();
        {
            let table = self.fast_tier.borrow();
            match table.get(idx) {
                Some(FastEntry::Compiled(_)) => {
                    return Some(std::cell::Ref::map(table, Vec::as_slice));
                }
                Some(FastEntry::Ineligible) => return None,
                _ => {}
            }
        }
        self.compile_fast_transitive(function);
        let table = self.fast_tier.borrow();
        if matches!(table.get(idx), Some(FastEntry::Compiled(_))) {
            Some(std::cell::Ref::map(table, Vec::as_slice))
        } else {
            None
        }
    }

    #[cold]
    #[inline(never)]
    fn compile_fast_transitive(&self, root: FunctionIndex) {
        use crate::fast_tier::{FastEntry, lower::lower_function};
        let mut queue = std::collections::VecDeque::new();
        queue.push_back((root, true));
        while let Some((func_idx, require_hot)) = queue.pop_front() {
            let idx = func_idx.index();
            if idx >= self.functions.len() || self.functions[idx].unit.0 == u32::MAX {
                continue;
            }
            {
                let table = self.fast_tier.borrow();
                if matches!(
                    table.get(idx),
                    Some(FastEntry::Compiled(_) | FastEntry::Ineligible)
                ) {
                    continue;
                }
            }
            let lowered = lower_function(self, func_idx, require_hot);
            let mut table = self.fast_tier.borrow_mut();
            if table.len() <= idx {
                table.resize(idx + 1, FastEntry::Unchecked);
            }
            self.has_fast_tier.set(true);
            match lowered {
                Some((fast, callees)) => {
                    table[idx] = FastEntry::Compiled(fast);
                    drop(table);
                    for callee in callees {
                        queue.push_back((callee, false));
                    }
                }
                None => {
                    table[idx] = FastEntry::Ineligible;
                }
            }
        }
    }

    /// Permanently disable Tier-2 entry for a function that repeatedly hits
    /// runtime type deopts (e.g. `SplFixedArray` or string-keyed maps).
    #[cold]
    #[inline(never)]
    pub(crate) fn mark_fast_ineligible(&self, function: FunctionIndex) {
        use crate::fast_tier::FastEntry;
        let idx = function.index();
        let mut table = self.fast_tier.borrow_mut();
        if table.len() <= idx {
            table.resize(idx + 1, FastEntry::Unchecked);
        }
        table[idx] = FastEntry::Ineligible;
        self.has_fast_tier.set(true);
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
        let mut sums = [0; 2];
        for loaded in &self.units {
            for (sum, bytes) in sums.iter_mut().zip(loaded.cache.fork_bytes()) {
                *sum += bytes;
            }
        }
        let [functions, closures] = sums;
        let sites = self.sites.borrow();
        let cells = sites
            .iter()
            .map(|entry| entry.as_deref().map_or(0, <[CachedSite]>::len))
            .sum::<usize>()
            * size_of::<CachedSite>();
        let boxes = sites.capacity() * size_of::<Option<Sites>>();
        vec![
            ("site cells", cells),
            ("per-function site boxes", boxes),
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
            sites: std::cell::RefCell::new(Vec::new()),
            has_local_sites: std::cell::Cell::new(false),
            base_sites: match &self.base_sites {
                Some(base) => Some(base.clone()),
                None => {
                    let local = self.sites.borrow();
                    if local.is_empty() {
                        None
                    } else {
                        Some(
                            local
                                .iter()
                                .map(|entry| entry.as_deref().map(crate::unit_caches::frozen_sites))
                                .collect(),
                        )
                    }
                }
            },
            trait_method_names: self.trait_method_names.clone(),
            imports: None,
            fast_tier: std::cell::RefCell::new(if self.has_fast_tier.get() {
                self.fast_tier.borrow().clone()
            } else {
                Vec::new()
            }),
            has_fast_tier: std::cell::Cell::new(self.has_fast_tier.get()),
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
