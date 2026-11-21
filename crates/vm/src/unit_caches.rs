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
    pub(crate) symbols: RefCell<Vec<Option<crate::calls::pending::LinkedFunction>>>,
    pub(crate) direct_natives: OnceCell<Box<[Cell<Option<crate::calls::sync_native::NativeLink>>]>>,
    pub(crate) constants: Rc<[phera_value::Value]>,
    pub(crate) class_names: Rc<[phera_value::Value]>,
    /// Keyed by immutable unit TypeId and made at first unit use; spelling cannot
    /// change, so scope classification needs no invalidation. Relative types resolve per call.
    pub(crate) type_scopes: Arc<[crate::types::DeclarationScope]>,
    pub(crate) class_types: Vec<RefCell<Option<ClassTypeEntry>>>,
    /// The site cells of the top-level code and of each function, one per
    /// instruction, made when the function first looks a site up.
    main_sites: Sites,
    function_sites: Box<[Sites]>,
    /// What each method site (`CallMethod`) of the top-level code and of each
    /// function found the last time it ran: one fixed entry per site, made
    /// when the function's first site runs.
    main_records: Records,
    function_records: Box<[Records]>,
    pub(crate) closure_signatures: RefCell<Vec<Option<crate::calls::call_state::Callee>>>,
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

/// The method site records of one function, one per site.
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
        let cells = (made(&self.main_sites) + self.function_sites.iter().map(made).sum::<usize>())
            * size_of::<RefCell<CachedSite>>();
        let boxes = (self.function_sites.len() + 1) * size_of::<Sites>();
        let types = self.class_types.len() * size_of::<RefCell<Option<ClassTypeEntry>>>();
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
            Some(function) => self.function_sites.get(function as usize)?,
            None => &self.main_sites,
        };
        sites
            .get_or_init(|| empty_sites(facts.instructions.len()))
            .get(ip)
    }

    /// The record of method site `site` of the function `facts` describes,
    /// which has `sites` method sites.
    pub(crate) fn site_record(
        &self,
        facts: &crate::function_facts::FunctionFacts,
        sites: usize,
        site: phera_bytecode::MethodSiteId,
    ) -> Option<&Cell<SiteRecord>> {
        let records = match facts.local {
            Some(function) => self.function_records.get(function as usize)?,
            None => &self.main_records,
        };
        records
            .get_or_init(|| (0..sites).map(|_| Cell::new(SiteRecord::EMPTY)).collect())
            .get(site.index())
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
            symbols: RefCell::new(vec![None; unit.symbols.len()]),
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
            class_types: unit.types.iter().map(|_| RefCell::new(None)).collect(),
            main_sites: Sites::new(),
            function_sites: unit.functions.iter().map(|_| Sites::new()).collect(),
            main_records: Records::new(),
            function_records: unit.functions.iter().map(|_| Records::new()).collect(),
            closure_signatures: RefCell::new(Vec::new()),
            class_names: unit
                .symbols
                .iter()
                .map(|symbol| phera_value::Value::from(symbol.written.as_str()))
                .collect(),
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

/// A fork keeps the caches warm: classes are shared, resolved callables
/// are copied (see `crate::snapshot`).
impl crate::snapshot::ForkCopy for UnitCache {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        // Only the functions that used a site before the snapshot have cells.
        let sites = |sites: &Sites, fork: &mut phera_value::Fork<'_>| {
            let copy = Sites::new();
            if let Some(cells) = sites.get() {
                let _ = copy.set(
                    cells
                        .iter()
                        .map(|site| RefCell::new(site.borrow().fork_copy(fork)))
                        .collect(),
                );
            }
            copy
        };
        let records = |records: &Records| {
            let copy = Records::new();
            if let Some(cells) = records.get() {
                let _ = copy.set(cells.iter().map(|cell| Cell::new(cell.get())).collect());
            }
            copy
        };
        UnitCache {
            trace_file: self.trace_file.clone(),
            names: self.names.clone(),
            symbols: RefCell::new(self.symbols.borrow().fork_copy(fork)),
            direct_natives: self.direct_natives.clone(),
            constants: self.constants.clone(),
            class_names: self.class_names.clone(),
            type_scopes: self.type_scopes.clone(),
            class_types: self.class_types.clone(),
            main_sites: sites(&self.main_sites, fork),
            function_sites: self
                .function_sites
                .iter()
                .map(|function| sites(function, fork))
                .collect(),
            main_records: records(&self.main_records),
            function_records: self.function_records.iter().map(records).collect(),
            closure_signatures: RefCell::new(self.closure_signatures.borrow().fork_copy(fork)),
        }
    }
}

impl crate::snapshot::ForkCopy for CachedSite {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        match self {
            CachedSite::Empty => CachedSite::Empty,
            CachedSite::Property(entry) => CachedSite::Property(entry.clone()),
            CachedSite::Function(linked) => CachedSite::Function(Box::new(linked.fork_copy(fork))),
        }
    }
}
