//! Request-local values made once per loaded file: names, literals, call
//! links and the per-instruction site caches.

use std::{cell::RefCell, rc::Rc};

use phera_bytecode::CompiledUnit;

/// Materialized immutable literals share PHP COW values across frame activations.
/// Keyed by constant/symbol index in a retained unit; populated at first unit use.
/// Bytecode literals never mutate, and VM-local ownership prevents cross-request reuse.
pub(crate) struct UnitCache {
    pub(crate) names: Vec<phera_value::PhpString>,
    pub(crate) symbols: RefCell<Vec<Option<crate::calls::call_state::Callee>>>,
    pub(crate) constants: Vec<phera_value::Value>,
    pub(crate) class_names: Vec<phera_value::Value>,
    pub(crate) classes: Vec<RefCell<Option<Rc<crate::classes::Class>>>>,
    /// Keyed by immutable unit TypeId and made at first unit use; spelling cannot
    /// change, so scope classification needs no invalidation. Relative types resolve per call.
    pub(crate) type_scopes: Vec<crate::types::DeclarationScope>,
    pub(crate) class_types: Vec<RefCell<Option<ClassTypeEntry>>>,
    main_sites: Vec<RefCell<CachedSite>>,
    function_sites: Vec<Vec<RefCell<CachedSite>>>,
}

/// Keyed by unit TypeId and guarded by receiver class identity. Entries are made
/// after a successful or failed absolute declaration check. Relative types bypass
/// this cache. Retained immutable classes cannot change ancestry or reuse an
/// identity; request-owned UnitCaches prevent declarations leaking across requests.
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
}

impl UnitCache {
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
            names: unit.names.iter().map(|name| name.as_str().into()).collect(),
            symbols: RefCell::new(vec![None; unit.symbols.len()]),
            constants: unit
                .constants
                .iter()
                .map(crate::calls::frame::value)
                .collect(),
            classes: unit.symbols.iter().map(|_| RefCell::new(None)).collect(),
            type_scopes: unit
                .types
                .iter()
                .map(crate::types::declaration_scope)
                .collect(),
            class_types: unit.types.iter().map(|_| RefCell::new(None)).collect(),
            main_sites: instruction_sites(&unit.main),
            function_sites: unit.functions.iter().map(instruction_sites).collect(),
            class_names: unit
                .symbols
                .iter()
                .map(|symbol| phera_value::Value::from(symbol.written.as_str()))
                .collect(),
        }
    }
}
