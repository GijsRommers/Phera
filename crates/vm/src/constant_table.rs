//! Resolve constant symbols once, invalidating fallbacks when PHP defines names.

use std::{collections::HashMap, sync::Arc};

use phera_bytecode::{CompiledUnit, SymbolId};
use phera_extension_api::NativeError;
use phera_value::Value;

/// A bytecode constant reference belongs to a particular immutable unit.
#[derive(Hash, PartialEq, Eq)]
struct ConstantSite {
    unit: usize,
    symbol: u32,
}

/// Retaining the code prevents pointer reuse from confusing cache identities.
struct ResolvedConstant {
    _unit: Arc<CompiledUnit>,
    value: Value,
}

/// Request constants and the resolved values of bytecode symbols.
pub(crate) struct ConstantTable {
    values: HashMap<String, Value>,
    resolved: HashMap<ConstantSite, ResolvedConstant>,
}

impl ConstantTable {
    pub(crate) fn new(mut values: HashMap<String, Value>) -> Self {
        for (name, number) in [
            ("EXTR_OVERWRITE", 0),
            ("EXTR_SKIP", 1),
            ("EXTR_PREFIX_SAME", 2),
            ("EXTR_PREFIX_ALL", 3),
            ("EXTR_PREFIX_INVALID", 4),
            ("EXTR_PREFIX_IF_EXISTS", 5),
            ("EXTR_IF_EXISTS", 6),
            ("EXTR_REFS", 256),
        ] {
            values.insert(name.into(), Value::Int(number));
        }
        Self {
            values,
            resolved: HashMap::new(),
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }

    pub(crate) fn contains_key(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    /// A new namespace constant can replace an earlier global fallback.
    pub(crate) fn insert(&mut self, name: String, value: Value) {
        self.resolved.clear();
        self.values.insert(name, value);
    }

    /// Failed fetches are never cached: a later define must remain observable.
    pub(crate) fn fetch(
        &mut self,
        unit: Arc<CompiledUnit>,
        symbol: SymbolId,
    ) -> Result<Value, NativeError> {
        let site = ConstantSite {
            unit: Arc::as_ptr(&unit) as usize,
            symbol: symbol.0,
        };
        if let Some(resolved) = self.resolved.get(&site) {
            return Ok(resolved.value.clone());
        }
        let definition = unit
            .symbols
            .get(symbol.index())
            .ok_or_else(|| internal_error!("constant symbol"))?;
        let value = self
            .values
            .get(&definition.name)
            .or_else(|| {
                definition
                    .fallback
                    .as_ref()
                    .and_then(|name| self.values.get(name))
            })
            .cloned()
            .ok_or_else(|| {
                NativeError::error(format!("Undefined constant \"{}\"", definition.name))
            })?;
        self.resolved.insert(
            site,
            ResolvedConstant {
                _unit: unit,
                value: value.clone(),
            },
        );
        Ok(value)
    }
}
