//! Per-unit call-symbol caches avoid resolving the same callee on every call.

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use phera_bytecode::CompiledUnit;

use crate::SymbolCache;

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
                symbols: Rc::new(RefCell::new(vec![None; unit.symbols.len()])),
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
