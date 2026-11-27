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
    trace_file: OnceCell<phera_value::Value>,
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
    /// What each constant symbol of the file (`FetchConstant`) last read,
    /// when that was a plain scalar: one cell per symbol, made at the
    /// file's first constant read in a request. Never copied from the
    /// snapshot.
    pub(crate) global_constants: OnceCell<Box<[Cell<Option<CachedConstant>>]>>,
    /// Where each name of the file that a `global $name` bound was found:
    /// the file-scope code and its register for the variable. One cell
    /// per name, made at the file's first `global` in a request.
    global_bindings: OnceCell<Box<[Cell<Option<GlobalBinding>>]>>,
    pub(crate) constants: Rc<[phera_value::Value]>,
    pub(crate) class_names: Rc<[phera_value::Value]>,
    /// Keyed by immutable unit TypeId and made at first unit use; spelling cannot
    /// change, so scope classification needs no invalidation. Relative types resolve per call.
    pub(crate) type_scopes: Arc<[crate::types::DeclarationScope]>,
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

/// A global constant's value as one `FetchConstant` site read it, valid
/// while the constant table is at the same epoch
/// ([`crate::constant_table::ConstantTable::epoch`]): no constant was
/// defined since, so the name still means the same constant. Only scalars
/// and interned strings, which are copied without a reference count, and
/// only constants without a deprecation, which PHP reports at every read.
#[derive(Clone, Copy)]
pub(crate) struct CachedConstant {
    pub(crate) epoch: u64,
    pub(crate) value: ScalarConstant,
}

/// A constant value the loop can store without touching a reference count.
#[derive(Clone, Copy)]
pub(crate) enum ScalarConstant {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Interned(&'static phera_value::InternedStr),
}

impl ScalarConstant {
    pub(crate) fn of(value: &Value) -> Option<Self> {
        Some(match value {
            Value::Null => Self::Null,
            Value::Bool(value) => Self::Bool(*value),
            Value::Int(value) => Self::Int(*value),
            Value::Float(value) => Self::Float(*value),
            Value::InternedStr(value) => Self::Interned(value),
            _ => return None,
        })
    }

    #[inline]
    pub(crate) fn value(self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Bool(value) => Value::Bool(value),
            Self::Int(value) => Value::Int(value),
            Self::Float(value) => Value::Float(value),
            Self::Interned(value) => Value::InternedStr(value),
        }
    }
}

impl UnitCache {
    /// Remembers what constant symbol `symbol` of this file read at `epoch`.
    pub(crate) fn remember_constant(&self, symbol: usize, epoch: u64, value: &Value) {
        let Some(value) = ScalarConstant::of(value) else {
            return;
        };
        let cells = self
            .global_constants
            .get_or_init(|| (0..self.symbol_count).map(|_| Cell::new(None)).collect());
        if let Some(cell) = cells.get(symbol) {
            cell.set(Some(CachedConstant { epoch, value }));
        }
    }

    /// The value constant symbol `symbol` read at `epoch`, if it is known.
    #[inline]
    pub(crate) fn known_constant(&self, symbol: usize, epoch: u64) -> Option<Value> {
        let cached = self.global_constants.get()?.get(symbol)?.get()?;
        (cached.epoch == epoch).then(|| cached.value.value())
    }
}

/// Where a global variable lives, as a `global $name` found it.
#[derive(Clone, Copy)]
pub(crate) enum GlobalBinding {
    /// A declared variable of file-scope code, which keeps its register for
    /// as long as the code runs. Valid while the innermost file-scope frame
    /// at the bottom of the stack runs `function`.
    Register {
        function: crate::code_table::FunctionIndex,
        register: phera_bytecode::Reg,
    },
    /// An entry of the request's globals table that no frame holds: valid
    /// while the entry at `index` is still the variable's and holds its
    /// value (`crate::globals::Globals::reference_at`).
    Table { index: u32 },
}

impl UnitCache {
    /// Remembers where `global $name` (name `name` of this file) found the
    /// variable.
    pub(crate) fn remember_global(&self, name: usize, binding: GlobalBinding) {
        let cells = self
            .global_bindings
            .get_or_init(|| (0..self.names.len()).map(|_| Cell::new(None)).collect());
        if let Some(cell) = cells.get(name) {
            cell.set(Some(binding));
        }
    }

    /// Where `global $name` last found the variable, if it ran before.
    #[inline]
    pub(crate) fn known_global(&self, name: usize) -> Option<GlobalBinding> {
        self.global_bindings.get()?.get(name)?.get()
    }
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
    /// A public PHP `__call(string $name, array $arguments)` handler invoked
    /// directly from the site when the class does not declare the method.
    MagicCall(crate::code_table::FunctionIndex),
    /// A public native getter with no parameters and a direct handler
    /// (`$method->getName()`): the site runs it and stores its value.
    Getter(Getter),
    /// A public native method with a direct handler and a few arguments
    /// (`$property->getValue($object)`): the site runs the handler on the
    /// arguments it finds in their registers and stores its value.
    Direct(Getter, crate::calls::native_signature::DirectMasks),
    /// Any other native method: the general sequence that follows prepares
    /// and calls it.
    Native,
    /// The VM prepares it (`__call`, an error, a closure, a generator).
    Vm,
}

// A record is copied out of its cell at every call of a method site: the
// outcome's variants, the direct one with the masks of its arguments
// included, stay in 16 bytes.
const _: () = assert!(std::mem::size_of::<SiteOutcome>() == 16);

impl SiteRecord {
    const EMPTY: SiteRecord = SiteRecord {
        class: 0,
        scope: None,
        outcome: SiteOutcome::Vm,
    };
}

/// How a cached static call (`self::`, `parent::`, or `Class::`) passes
/// `$this` or late-static-binding `called` class.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StaticCallMode {
    /// Non-forwarding static method call (`Class::foo()`): called class is `called`.
    Static,
    /// Forwarding static method call (`self::foo()` / `parent::foo()`): forwards
    /// the caller's called class if present, else `called`.
    Forward,
    /// Instance method invoked through `self::foo()` / `parent::foo()` on `$this`.
    Instance,
}

/// Cached resolution for an `InitStaticCall` site whose target class is fixed
/// (`self::`, `parent::`, or a named class — never `static::` and never inside a closure).
#[derive(Clone, Copy)]
pub(crate) struct StaticCallCache {
    pub(crate) function: crate::code_table::FunctionIndex,
    pub(crate) called: crate::classes::ClassIndex,
    pub(crate) mode: StaticCallMode,
}

/// One cache cell per unit/function/instruction, not a hash lookup per access.
/// Cells start empty at first unit use. Their payload is made after successful
/// resolution; class/scope guards in each payload replace polymorphic entries.
/// Units are retained and immutable, so slot identity cannot be reused or missed.
pub(crate) enum CachedSite {
    Empty,
    Property(Rc<crate::property_sites::PropertyEntry>),
    /// What an unqualified built-in call in a namespace found when it first
    /// ran (see `crate::builtin_guards`).
    BuiltinGuard(crate::builtin_guards::Resolution),
    Function(Box<crate::calls::pending::LinkedFunction>),
    StaticCall(StaticCallCache),
}

const _: () = assert!(std::mem::size_of::<CachedSite>() == 16);

impl UnitCache {
    /// What a fork's copy of this cache holds on the heap, in bytes: the
    /// per-symbol and closure resolution slots. The shared literal tables
    /// cost a fork nothing. See `crate::snapshot::footprint`.
    pub(crate) fn fork_bytes(&self) -> [usize; 2] {
        use std::mem::size_of;
        let functions = self.symbols.borrow().len()
            * size_of::<Option<crate::calls::pending::LinkedFunction>>();
        let closures = self.closure_signatures.borrow().len()
            * size_of::<Option<crate::calls::call_state::Callee>>();
        [functions, closures]
    }

    pub(crate) fn trace_file(&self, unit: &CompiledUnit) -> Value {
        self.trace_file
            .get_or_init(|| Value::interned(unit.file.as_bytes()))
            .clone()
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
    #[inline(never)]
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

/// The site cells of one function, made when it first writes one: a request
/// loads every function of a file but runs few of them, so most functions
/// never allocate any, and forks read immutable snapshot sites directly from
/// `base_sites` without copying or borrowing a `RefCell`.
pub(crate) type Sites = Box<[CachedSite]>;

/// An empty cell per instruction, and one past the last: a site is keyed by
/// the position after its instruction. What a site resolves to is allocated
/// when it is first written.
#[cold]
pub(crate) fn empty_sites(instructions: usize) -> Sites {
    (0..=instructions).map(|_| CachedSite::Empty).collect()
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
            global_constants: OnceCell::new(),
            global_bindings: OnceCell::new(),
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
                .map(|symbol| phera_value::Value::interned(symbol.written.as_bytes()))
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
                        array.insert(phera_value::ArrayKey::interned(key), value);
                    }
                }
            }
            Value::Array(Rc::new(array))
        }
        Constant::Bool(value) => Value::Bool(*value),
        Constant::Int(value) => Value::Int(*value),
        Constant::Float(value) => Value::Float(*value),
        Constant::String(value) => Value::interned(value),
    }
}

/// How many cells and links of a file's cache a request can use
/// without making them, which is a snapshot's decision: see
/// [`UnitCache`]'s `base`.
impl UnitCache {
    /// Clears request-local mutable caches on a kept snapshot unit when a
    /// dirty fork rolls back its `CodeTable` for recycling.
    pub(crate) fn reset_mutable_cells(&mut self) {
        let symbols = self.symbols.get_mut();
        if !symbols.is_empty() {
            symbols.clear();
        }
        self.records.take();
        self.global_constants.take();
        self.global_bindings.take();
        let closures = self.closure_signatures.get_mut();
        if !closures.is_empty() {
            closures.clear();
        }
    }

    /// A cache for the same file with nothing filled: the values made from
    /// the file are shared, every cell is empty.
    fn without_cells(&self) -> Self {
        UnitCache {
            trace_file: self.trace_file.clone(),
            names: self.names.clone(),
            symbols: RefCell::new(Vec::new()),
            symbol_count: self.symbol_count,
            direct_natives: OnceCell::new(),
            global_constants: OnceCell::new(),
            global_bindings: OnceCell::new(),
            constants: self.constants.clone(),
            class_names: self.class_names.clone(),
            type_scopes: self.type_scopes.clone(),
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
        copy
    }
}

/// The cells of one function that a request forked from the snapshot may
/// use: the cells that were filled, without those that hold an object of
/// the VM they were made in.
pub(crate) fn frozen_sites(sites: &[CachedSite]) -> Sites {
    sites.iter().map(CachedSite::shared).collect()
}

impl CachedSite {
    /// What a request forked from the snapshot may take from this cell:
    /// everything that holds no object of the VM it was made in.
    pub(crate) fn shared(&self) -> CachedSite {
        match self {
            CachedSite::Empty => CachedSite::Empty,
            CachedSite::Property(entry) => CachedSite::Property(entry.clone()),
            CachedSite::BuiltinGuard(resolution) => CachedSite::BuiltinGuard(*resolution),
            CachedSite::Function(linked) => match linked.shared() {
                Some(linked) => CachedSite::Function(Box::new(linked)),
                None => CachedSite::Empty,
            },
            CachedSite::StaticCall(entry) => CachedSite::StaticCall(*entry),
        }
    }
}
