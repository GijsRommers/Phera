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
    pub(crate) classes: Vec<RefCell<Option<Rc<crate::classes::Class>>>>,
    /// Keyed by immutable unit TypeId and made at first unit use; spelling cannot
    /// change, so scope classification needs no invalidation. Relative types resolve per call.
    pub(crate) type_scopes: Arc<[crate::types::DeclarationScope]>,
    pub(crate) class_types: Vec<RefCell<Option<ClassTypeEntry>>>,
    main_sites: Vec<RefCell<CachedSite>>,
    function_sites: Vec<Vec<RefCell<CachedSite>>>,
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
    pub(crate) fn trace_file(&self, unit: &CompiledUnit) -> Value {
        Value::String(
            self.trace_file
                .get_or_init(|| unit.file.as_str().into())
                .clone(),
        )
    }

    /// IP has already advanced; the extra terminal cell covers the last instruction.
    pub(crate) fn site(&self, function: Option<usize>, ip: usize) -> Option<&RefCell<CachedSite>> {
        match function {
            Some(function) => self.function_sites.get(function)?.get(ip),
            None => self.main_sites.get(ip),
        }
    }
}

/// Reserve only a small empty cell per instruction; resolved payloads allocate on first use.
fn instruction_sites(function: &phera_bytecode::Function) -> Vec<RefCell<CachedSite>> {
    (0..=function.instructions.len())
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
            classes: unit.symbols.iter().map(|_| RefCell::new(None)).collect(),
            type_scopes: match &unit.metadata {
                Some(metadata) => metadata.type_scopes.clone(),
                None => unit
                    .types
                    .iter()
                    .map(crate::types::DeclarationScope::of)
                    .collect(),
            },
            class_types: unit.types.iter().map(|_| RefCell::new(None)).collect(),
            main_sites: instruction_sites(&unit.main),
            function_sites: unit.functions.iter().map(instruction_sites).collect(),
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
        let sites = |sites: &Vec<RefCell<CachedSite>>, fork: &mut phera_value::Fork<'_>| {
            sites
                .iter()
                .map(|site| RefCell::new(site.borrow().fork_copy(fork)))
                .collect::<Vec<_>>()
        };
        UnitCache {
            trace_file: self.trace_file.clone(),
            names: self.names.clone(),
            symbols: RefCell::new(self.symbols.borrow().fork_copy(fork)),
            direct_natives: self.direct_natives.clone(),
            constants: self.constants.clone(),
            class_names: self.class_names.clone(),
            classes: self.classes.clone(),
            type_scopes: self.type_scopes.clone(),
            class_types: self.class_types.clone(),
            main_sites: sites(&self.main_sites, fork),
            function_sites: self
                .function_sites
                .iter()
                .map(|function| sites(function, fork))
                .collect(),
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
