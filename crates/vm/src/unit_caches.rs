//! Per-unit call-symbol caches avoid resolving the same callee on every call.

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use phera_bytecode::CompiledUnit;

use crate::SymbolCache;

/// Cache identity follows the compiled unit allocation for the current request.
pub(crate) struct UnitCaches {
    symbols: HashMap<usize, SymbolCache>,
}

impl UnitCaches {
    pub(crate) fn new() -> Self {
        Self {
            symbols: HashMap::new(),
        }
    }

    /// Frames share symbol slots so recursive calls reuse the resolved signatures.
    pub(crate) fn symbols(&mut self, unit: &Arc<CompiledUnit>) -> SymbolCache {
        self.symbols
            .entry(Arc::as_ptr(unit) as usize)
            .or_insert_with(|| Rc::new(RefCell::new(vec![None; unit.symbols.len()])))
            .clone()
    }
}
