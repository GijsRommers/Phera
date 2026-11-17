//! Per-unit call-symbol caches avoid resolving the same callee on every call.

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use phera_bytecode::CompiledUnit;

use crate::SymbolCache;

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
    /// Keyed by immutable function index, made at first unit use. The count is
    /// records the last required slot; bytecode cannot change.
    pub(crate) required_parameters: Vec<usize>,
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
    Method(Box<crate::classes::method_cache::MethodEntry>),
    Static(Box<crate::classes::static_sites::StaticEntry>),
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

/// Cache identity follows the compiled unit allocation for the current request.
pub(crate) struct UnitCaches {
    symbols: HashMap<usize, UnitSymbols>,
}

/// Retaining the unit prevents allocator address reuse from changing cache identity.
struct UnitSymbols {
    unit: Arc<CompiledUnit>,
    symbols: SymbolCache,
}

impl UnitCaches {
    pub(crate) fn new() -> Self {
        Self {
            symbols: HashMap::new(),
        }
    }

    /// Frames share symbol slots so recursive calls reuse the resolved signatures.
    pub(crate) fn symbols(&mut self, unit: &Arc<CompiledUnit>) -> SymbolCache {
        let entry = self
            .symbols
            .entry(Arc::as_ptr(unit) as usize)
            .or_insert_with(|| UnitSymbols {
                unit: unit.clone(),
                symbols: Rc::new(UnitCache {
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
                    required_parameters: unit
                        .functions
                        .iter()
                        .map(|function| {
                            function
                                .parameters
                                .iter()
                                .rposition(|parameter| {
                                    !parameter.has_default && !parameter.variadic
                                })
                                .map_or(0, |index| index + 1)
                        })
                        .collect(),
                    class_types: unit.types.iter().map(|_| RefCell::new(None)).collect(),
                    main_sites: instruction_sites(&unit.main),
                    function_sites: unit.functions.iter().map(instruction_sites).collect(),
                    class_names: unit
                        .symbols
                        .iter()
                        .map(|symbol| phera_value::Value::from(symbol.written.as_str()))
                        .collect(),
                }),
            });
        debug_assert!(Arc::ptr_eq(&entry.unit, unit));
        entry.symbols.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::UnitCaches;
    use std::sync::Arc;

    /// Cached symbols must retain their allocation identity after all frames return.
    #[test]
    fn cached_units_stay_alive() {
        let unit = Arc::new(phera_compiler::compile("<?php", "cache.php").unwrap());
        let weak = Arc::downgrade(&unit);
        let mut caches = UnitCaches::new();
        caches.symbols(&unit);
        drop(unit);
        assert!(weak.upgrade().is_some());
    }
}
