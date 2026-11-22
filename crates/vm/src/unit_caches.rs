//! Request-local values made once per loaded file: names, literals, call
//! links and the per-instruction site caches.

use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::Rc,
    sync::Arc,
};

use phera_bytecode::{CompiledUnit, Constant};
use phera_value::{PhpArray, Value};

/// Materialized immutable literals share PHP COW values across frame activations.
/// Keyed by constant/symbol index in a retained unit; populated at first unit use.
/// Bytecode literals never mutate, and VM-local ownership prevents cross-request reuse.
pub(crate) struct UnitCache {
    /// Throwable properties and trace entries share the unit's immutable filename.
    trace_file: OnceCell<phera_value::PhpString>,
    /// The file's names, literals and class names as PHP values: made once
    /// when the file is loaded, never changed, and shared by every fork of
    /// a snapshot that holds the file.
    pub(crate) names: Rc<[phera_value::PhpString]>,
    /// What each function symbol of the file was linked to, once its first call
    /// resolved it.
    ///
    /// Empty until a symbol of the file is linked in this request: the
    /// links the snapshot made are in `base`, and a link is copied here
    /// when it is first asked for (see [`UnitCache::base_symbol`]).
    pub(crate) symbols: RefCell<Vec<Option<crate::calls::pending::LinkedFunction>>>,
    /// How many function symbols the file has.
    symbol_count: usize,
    pub(crate) direct_natives: OnceCell<Box<[Cell<Option<crate::calls::sync_native::NativeLink>>]>>,
    pub(crate) constants: Rc<[phera_value::Value]>,
    pub(crate) class_names: Rc<[phera_value::Value]>,
    /// Keyed by immutable unit TypeId and made at first unit use; spelling cannot
    /// change, so scope classification needs no invalidation. Relative types resolve per call.
    pub(crate) type_scopes: Arc<[crate::types::DeclarationScope]>,
    pub(crate) class_types: ClassTypes,
    /// The site cells of the top-level code and of each function, one per
    /// instruction, made when the function first looks a site up.
    main_sites: Sites,
    /// The cells of each function, made when the file's first function uses
    /// one: a request runs few of its files' functions, and a fork that
    /// made a box per function of every loaded file would pay for them all.
    function_sites: OnceCell<Box<[Sites]>>,
    /// What each method site (`CallMethod`) of the file found the last time
    /// it ran: one flat array for the whole file, one fixed entry per site,
    /// made when the first of the file's sites runs, so a file whose sites
    /// never run in a request costs nothing. A function's sites start at
    /// its entry in `record_starts`.
    records: Records,
    /// Where each function's sites start in `records`: the top-level code's
    /// at 0, function `i`'s at `record_starts[i]`, and the total last. Made
    /// with the file, shared by every fork.
    record_starts: Rc<[u32]>,
    pub(crate) closure_signatures: RefCell<Vec<Option<crate::calls::call_state::Callee>>>,
    /// What the snapshot this cache was forked from had filled in the same
    /// file, which this request reads and never changes: a cell of this
    /// cache that is empty is made from the snapshot's cell when it is
    /// first needed, never copied before. `None` in the snapshot's own
    /// cache and in a cache that was never forked.
    base: Option<Rc<UnitCache>>,
}

/// The per-type class checks of one file, one cell per declared type.
///
/// A file loaded by a request has them from the start. A fork's start as an
/// empty array, which costs nothing to make and which the loop reads exactly
/// as it reads a full one (one bounds check, no test for "not made yet");
/// the first time the file checks a class type in slow path, with the code
/// table borrowed mutably, [`ClassTypes::make`] fills it with the snapshot's
/// answers (or empty ones).
pub(crate) struct ClassTypes {
    cells: Box<[RefCell<Option<ClassTypeEntry>>]>,
    /// How many types the file declares.
    count: usize,
    /// The snapshot's cache of the file, whose answers the cells start with.
    base: Option<Rc<UnitCache>>,
}

impl ClassTypes {
    /// The cells of a file made now, with no answers.
    fn made(count: usize) -> Self {
        ClassTypes {
            cells: (0..count).map(|_| RefCell::new(None)).collect(),
            count,
            base: None,
        }
    }

    /// The cells of a fork's file: not made yet.
    fn not_made(count: usize) -> Self {
        ClassTypes {
            cells: Box::default(),
            count,
            base: None,
        }
    }

    /// The check of declared type `index`, if the cells are made.
    #[inline(always)]
    pub(crate) fn get(&self, index: usize) -> Option<&RefCell<Option<ClassTypeEntry>>> {
        self.cells.get(index)
    }

    /// Makes the cells if they are not, with the answers the snapshot found
    /// out: they hold in a fork, which shares the snapshot's classes (an
    /// answer says "objects of this class satisfy this type", and is used
    /// only for an object of that class).
    pub(crate) fn make(&mut self) {
        if !self.cells.is_empty() || self.count == 0 {
            return;
        }
        let known = self
            .base
            .as_ref()
            .map(|base| &base.class_types.cells)
            .filter(|cells| cells.len() == self.count);
        self.cells = match known {
            Some(cells) => cells
                .iter()
                .map(|cell| RefCell::new(cell.borrow().clone()))
                .collect(),
            None => (0..self.count).map(|_| RefCell::new(None)).collect(),
        };
    }
}

/// Keyed by unit TypeId and guarded by receiver class identity. Entries are made
/// after a successful or failed absolute declaration check. Relative types bypass
/// this cache. Retained immutable classes cannot change ancestry or reuse an
/// identity; request-owned UnitCaches prevent declarations leaking across requests.
#[derive(Clone)]
pub(crate) struct ClassTypeEntry {
    pub(crate) class: Rc<dyn phera_value::ClassHandle>,
    pub(crate) matches: bool,
}

/// A native getter's direct handler (`MethodEntry::direct`).
pub(crate) type Getter =
    fn(&phera_value::Object, &[Option<phera_value::Value>]) -> Option<phera_value::Value>;

/// The method site records of one file, one per site.
pub(crate) type Records = OnceCell<Box<[Cell<SiteRecord>]>>;

/// What a method site found for a receiver of class `class`: the method
/// depends only on the class and on the scope the site is written in, so
/// the next call with a receiver of the same class needs no lookup. A
/// receiver of another class looks the method up and replaces the record.
///
/// The class is held as the address of the receiver's class handle, so
/// that a hit is one comparison. The record also holds the class scope it
/// was made in: the composed copies of a trait method share their records
/// but run in different scopes, and what a scope may call differs. A VM frees no class while it runs (the
/// class tables hold every class it linked until the VM ends, and the
/// records die with the VM), so an address cannot come to mean another
/// class.
#[derive(Clone, Copy)]
pub(crate) struct SiteRecord {
    /// 0 for a record not made yet.
    pub(crate) class: usize,
    /// The class scope of the code the record was made for.
    pub(crate) scope: Option<crate::classes::ClassIndex>,
    pub(crate) outcome: SiteOutcome,
}

impl SiteRecord {
    /// The address that identifies `class` in a record.
    #[inline(always)]
    pub(crate) fn address(class: &std::rc::Rc<dyn phera_value::ClassHandle>) -> usize {
        std::rc::Rc::as_ptr(class).cast::<()>() as usize
    }
}

/// How a method site calls the method it found.
#[derive(Clone, Copy)]
pub(crate) enum SiteOutcome {
    /// A method written in PHP the site's scope may call: entered from the
    /// site.
    Enter(crate::code_table::FunctionIndex),
    /// A public native getter with no parameters and a direct handler
    /// (`$method->getName()`): the site runs it and stores its value.
    Getter(Getter),
    /// Any other native method: the general sequence that follows prepares
    /// and calls it.
    Native,
    /// The VM prepares it (`__call`, an error, a closure, a generator).
    Vm,
}

impl SiteRecord {
    const EMPTY: SiteRecord = SiteRecord {
        class: 0,
        scope: None,
        outcome: SiteOutcome::Vm,
    };
}

/// One cache cell per unit/function/instruction, not a hash lookup per access.
/// Cells start empty at first unit use. Their payload is made after successful
/// resolution; class/scope guards in each payload replace polymorphic entries.
/// Units are retained and immutable, so slot identity cannot be reused or missed.
pub(crate) enum CachedSite {
    Empty,
    Property(Rc<crate::property_sites::PropertyEntry>),
    /// A static property read (see `crate::static_sites`).
    Static(Rc<crate::static_sites::StaticEntry>),
    Function(Box<crate::calls::pending::LinkedFunction>),
}

impl UnitCache {
    /// What a fork's copy of this cache holds on the heap, in bytes: the
    /// site cells made so far, the per-function cell boxes, and the
    /// per-symbol and per-type resolution slots. The shared literal tables
    /// cost a fork nothing. See `crate::snapshot::footprint`.
    pub(crate) fn fork_bytes(&self) -> [usize; 5] {
        use std::mem::size_of;
        let made = |sites: &Sites| sites.get().map_or(0, |cells| cells.len());
        let cells = (made(&self.main_sites)
            + self
                .function_sites
                .get()
                .map_or(0, |boxes| boxes.iter().map(made).sum::<usize>()))
            * size_of::<RefCell<CachedSite>>();
        let boxes =
            (self.function_sites.get().map_or(0, |boxes| boxes.len()) + 1) * size_of::<Sites>();
        let types = self.class_types.cells.len() * size_of::<RefCell<Option<ClassTypeEntry>>>();
        let functions = self.symbols.borrow().len()
            * size_of::<Option<crate::calls::pending::LinkedFunction>>();
        let closures = self.closure_signatures.borrow().len()
            * size_of::<Option<crate::calls::call_state::Callee>>();
        [cells, boxes, types, functions, closures]
    }

    pub(crate) fn trace_file(&self, unit: &CompiledUnit) -> Value {
        Value::String(
            self.trace_file
                .get_or_init(|| unit.file.as_str().into())
                .clone(),
        )
    }

    /// The site of the instruction of the function `facts` describes that
    /// ends at `ip`: the VM looks it up after the instruction pointer has
    /// advanced.
    #[inline]
    pub(crate) fn site(
        &self,
        facts: &crate::function_facts::FunctionFacts,
        ip: usize,
    ) -> Option<&RefCell<CachedSite>> {
        let sites = match facts.local {
            Some(function) => match self.function_sites.get() {
                Some(boxes) => boxes.get(function as usize)?,
                None => return self.first_site(facts, ip),
            },
            None => &self.main_sites,
        };
        sites.get_or_init(|| self.make_sites(facts)).get(ip)
    }

    /// The cells of a function that first looks a site up: those the
    /// snapshot filled that a request may use, else empty ones.
    #[cold]
    fn make_sites(
        &self,
        facts: &crate::function_facts::FunctionFacts,
    ) -> Box<[RefCell<CachedSite>]> {
        let wanted = facts.instructions.len() + 1;
        let known = self
            .base
            .as_ref()
            .and_then(|base| base.sites_of(facts.local));
        match known {
            Some(cells) if cells.len() == wanted => cells
                .iter()
                .map(|cell| RefCell::new(cell.borrow().shared()))
                .collect(),
            _ => empty_sites(facts.instructions.len()),
        }
    }

    /// The site, when the file has not made the box of any function's
    /// cells yet: makes it.
    #[cold]
    #[inline(never)]
    fn first_site(
        &self,
        facts: &crate::function_facts::FunctionFacts,
        ip: usize,
    ) -> Option<&RefCell<CachedSite>> {
        let boxes = self
            .function_sites
            .get_or_init(|| self.function_site_boxes());
        let sites = boxes.get(facts.local? as usize)?;
        sites.get_or_init(|| self.make_sites(facts)).get(ip)
    }

    /// A box for the cells of each function of the file, all empty.
    #[cold]
    fn function_site_boxes(&self) -> Box<[Sites]> {
        // `record_starts` has an entry for each function and the total.
        let functions = self.record_starts.len().saturating_sub(1);
        (0..functions).map(|_| Sites::new()).collect()
    }

    /// The cells a function has made, if it has.
    fn sites_of(&self, local: Option<u32>) -> Option<&[RefCell<CachedSite>]> {
        let sites = match local {
            Some(function) => self.function_sites.get()?.get(function as usize)?,
            None => &self.main_sites,
        };
        sites.get().map(|cells| &**cells)
    }

    /// The record of method site `site` of the function `facts` describes,
    /// which has `sites` method sites.
    pub(crate) fn site_record(
        &self,
        facts: &crate::function_facts::FunctionFacts,
        sites: usize,
        site: phera_bytecode::MethodSiteId,
    ) -> Option<&Cell<SiteRecord>> {
        let start = match facts.local {
            Some(function) => *self.record_starts.get(function as usize)? as usize,
            None => 0,
        };
        if site.index() >= sites {
            return None;
        }
        let total = self.record_starts.last().copied().unwrap_or(0) as usize;
        self.records
            .get_or_init(|| self.make_records(total))
            .get(start + site.index())
    }

    /// The records of a file whose first method site runs: a copy of the
    /// snapshot's, which stay true in every fork (the classes they name are
    /// the snapshot's, which a fork shares), or empty ones.
    #[cold]
    fn make_records(&self, total: usize) -> Box<[Cell<SiteRecord>]> {
        let known = self.base.as_ref().and_then(|base| base.records.get());
        match known {
            Some(cells) if cells.len() == total => {
                cells.iter().map(|cell| Cell::new(cell.get())).collect()
            }
            _ => (0..total).map(|_| Cell::new(SiteRecord::EMPTY)).collect(),
        }
    }
}

/// The site cells of one function, made when it first uses one: a request
/// loads every function of a file but runs few of them, so most functions
/// never get any (and a fork of a snapshot copies only those that ran).
pub(crate) type Sites = OnceCell<Box<[RefCell<CachedSite>]>>;

/// An empty cell per instruction, and one past the last: a site is keyed by
/// the position after its instruction. What a site resolves to is allocated
/// when it is first used.
#[cold]
fn empty_sites(instructions: usize) -> Box<[RefCell<CachedSite>]> {
    (0..=instructions)
        .map(|_| RefCell::new(CachedSite::Empty))
        .collect()
}

impl UnitCache {
    /// Make the request-local values of a file when the code table loads it.
    pub(crate) fn new(unit: &CompiledUnit) -> Self {
        Self {
            trace_file: OnceCell::new(),
            names: unit.names.iter().map(|name| name.as_str().into()).collect(),
            symbols: RefCell::new(Vec::new()),
            symbol_count: unit.symbols.len(),
            direct_natives: OnceCell::new(),
            constants: unit
                .constants
                .iter()
                .map(crate::unit_caches::value)
                .collect(),
            type_scopes: match &unit.metadata {
                Some(metadata) => metadata.type_scopes.clone(),
                None => unit
                    .types
                    .iter()
                    .map(crate::types::DeclarationScope::of)
                    .collect(),
            },
            class_types: ClassTypes::made(unit.types.len()),
            main_sites: Sites::new(),
            function_sites: OnceCell::new(),
            records: Records::new(),
            record_starts: {
                let mut next = unit.main.method_sites.len() as u32;
                let mut starts = Vec::with_capacity(unit.functions.len() + 1);
                for function in &unit.functions {
                    starts.push(next);
                    next += function.method_sites.len() as u32;
                }
                starts.push(next);
                starts.into()
            },
            closure_signatures: RefCell::new(Vec::new()),
            class_names: unit
                .symbols
                .iter()
                .map(|symbol| phera_value::Value::from(symbol.written.as_str()))
                .collect(),
            base: None,
        }
    }
}

/// Materialize bytecode constants as request-local PHP values.
pub(crate) fn value(constant: &Constant) -> Value {
    match constant {
        Constant::Null => Value::Null,
        Constant::Array(entries) => {
            let mut array = PhpArray::new();
            for (key, constant) in entries {
                let value = value(constant);
                match key {
                    phera_bytecode::ConstantKey::Next => {
                        let _ = array.push(value);
                    }
                    phera_bytecode::ConstantKey::Int(key) => {
                        array.insert(phera_value::ArrayKey::Int(*key), value);
                    }
                    phera_bytecode::ConstantKey::String(key) => {
                        array.insert(phera_value::ArrayKey::String(key.clone().into()), value);
                    }
                }
            }
            Value::Array(Rc::new(array))
        }
        Constant::Bool(value) => Value::Bool(*value),
        Constant::Int(value) => Value::Int(*value),
        Constant::Float(value) => Value::Float(*value),
        Constant::String(value) => Value::from(value.clone()),
    }
}

/// How many cells and links of a file's cache a request can use
/// without making them, which is a snapshot's decision: see
/// [`UnitCache`]'s `base`.
impl UnitCache {
    /// A cache for the same file with nothing filled: the values made from
    /// the file are shared, every cell is empty.
    fn without_cells(&self) -> Self {
        UnitCache {
            trace_file: self.trace_file.clone(),
            names: self.names.clone(),
            symbols: RefCell::new(Vec::new()),
            symbol_count: self.symbol_count,
            direct_natives: OnceCell::new(),
            constants: self.constants.clone(),
            class_names: self.class_names.clone(),
            type_scopes: self.type_scopes.clone(),
            class_types: ClassTypes::not_made(self.class_types.count),
            main_sites: Sites::new(),
            function_sites: OnceCell::new(),
            records: Records::new(),
            record_starts: self.record_starts.clone(),
            closure_signatures: RefCell::new(Vec::new()),
            base: None,
        }
    }

    /// The cells of this cache that a request forked from the snapshot may
    /// use, in a cache of their own: everything that holds no object of the
    /// VM it was made in.
    fn frozen(&self) -> Self {
        let mut frozen = self.without_cells();
        frozen.main_sites = frozen_sites(&self.main_sites);
        if let Some(boxes) = self.function_sites.get() {
            let _ = frozen
                .function_sites
                .set(boxes.iter().map(frozen_sites).collect());
        }
        if let Some(cells) = self.records.get() {
            let _ = frozen
                .records
                .set(cells.iter().map(|cell| Cell::new(cell.get())).collect());
        }
        *frozen.symbols.get_mut() = self
            .symbols
            .borrow()
            .iter()
            .map(|linked| linked.as_ref().and_then(|linked| linked.shared()))
            .collect();
        if let Some(cells) = self.direct_natives.get() {
            let _ = frozen
                .direct_natives
                .set(cells.iter().map(|cell| Cell::new(cell.get())).collect());
        }
        frozen.class_types.cells = self
            .class_types
            .cells
            .iter()
            .map(|cell| RefCell::new(cell.borrow().clone()))
            .collect();
        *frozen.closure_signatures.get_mut() = self
            .closure_signatures
            .borrow()
            .iter()
            .map(|callee| {
                callee
                    .as_ref()
                    .and_then(crate::calls::call_state::Callee::shared)
            })
            .collect();
        frozen
    }

    /// What the snapshot linked function symbol `symbol` of this file to,
    /// stored in this request's cache as well so that the loop reads it
    /// from there from now on.
    pub(crate) fn base_symbol(
        &self,
        symbol: usize,
    ) -> Option<crate::calls::pending::LinkedFunction> {
        let linked = self
            .base
            .as_ref()?
            .symbols
            .borrow()
            .get(symbol)?
            .as_ref()?
            .clone();
        self.set_symbol(symbol, linked.clone());
        Some(linked)
    }

    /// Remembers what function symbol `symbol` of this file is linked to.
    pub(crate) fn set_symbol(&self, symbol: usize, linked: crate::calls::pending::LinkedFunction) {
        let mut symbols = self.symbols.borrow_mut();
        if symbols.len() < self.symbol_count {
            symbols.resize_with(self.symbol_count, || None);
        }
        if let Some(slot) = symbols.get_mut(symbol) {
            *slot = Some(linked);
        }
    }

    /// The direct links of this file's native calls, made at the first one,
    /// with the snapshot's.
    pub(crate) fn direct_native_cells(
        &self,
    ) -> &[Cell<Option<crate::calls::sync_native::NativeLink>>] {
        self.direct_natives.get_or_init(|| {
            let known = self
                .base
                .as_ref()
                .and_then(|base| base.direct_natives.get());
            match known {
                Some(cells) => cells.iter().map(|cell| Cell::new(cell.get())).collect(),
                None => (0..self.symbol_count).map(|_| Cell::new(None)).collect(),
            }
        })
    }

    /// The signature a closure expression of this file made in the snapshot.
    pub(crate) fn base_closure_signature(
        &self,
        local: usize,
    ) -> Option<crate::calls::call_state::Callee> {
        self.base
            .as_ref()?
            .closure_signatures
            .borrow()
            .get(local)?
            .clone()
    }
}

/// What a fork does with the caches of its snapshot: the first copy of a
/// cache (the boot's, which becomes the snapshot) keeps what a request may
/// use as `base`; a fork of the snapshot starts with empty cells and the
/// same `base`. Nothing a request fills is ever in `base`.
impl crate::snapshot::ForkCopy for UnitCache {
    fn fork_copy(&self, _: &mut phera_value::Fork<'_>) -> Self {
        let mut copy = self.without_cells();
        copy.base = match &self.base {
            Some(base) => Some(base.clone()),
            None => Some(Rc::new(self.frozen())),
        };
        if copy.base.is_some() {
            copy.class_types.base = copy.base.clone();
        }
        copy
    }
}

/// The cells of one function that a request forked from the snapshot may
/// use: the cells that were filled, without those that hold an object of
/// the VM they were made in.
fn frozen_sites(sites: &Sites) -> Sites {
    let copy = Sites::new();
    if let Some(cells) = sites.get() {
        let _ = copy.set(
            cells
                .iter()
                .map(|site| RefCell::new(site.borrow().shared()))
                .collect(),
        );
    }
    copy
}

impl CachedSite {
    /// What a request forked from the snapshot may take from this cell:
    /// everything that holds no object of the VM it was made in.
    fn shared(&self) -> CachedSite {
        match self {
            CachedSite::Empty => CachedSite::Empty,
            CachedSite::Property(entry) => CachedSite::Property(entry.clone()),
            CachedSite::Static(entry) => CachedSite::Static(entry.clone()),
            CachedSite::Function(linked) => match linked.shared() {
                Some(linked) => CachedSite::Function(Box::new(linked)),
                None => CachedSite::Empty,
            },
        }
    }
}
