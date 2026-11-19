//! Resolve constant symbols once, invalidating fallbacks when PHP defines names.

use crate::code_table::FunctionIndex;
use std::{collections::HashMap, rc::Rc, sync::Arc};

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

#[derive(Clone)]
pub(crate) struct GlobalConstant {
    pub(crate) value: Value,
    pub(crate) deprecated: Option<Rc<str>>,
}

/// Request constants and the resolved values of bytecode symbols.
pub(crate) struct ConstantTable {
    native: Rc<NativeConstants>,
    values: HashMap<String, GlobalConstant>,
    user_order: Vec<String>,
    origins: HashMap<String, ConstantOrigin>,
    resolved: HashMap<ConstantSite, ResolvedConstant>,
}

pub(crate) type NativeConstants = HashMap<String, GlobalConstant>;

pub(crate) fn native_constants(registry: &phera_extension_api::Registry) -> NativeConstants {
    let mut values: NativeConstants = registry
        .constant_entries()
        .map(|(name, value, deprecated)| {
            (
                name.to_owned(),
                GlobalConstant {
                    value: value.to_value(),
                    deprecated: deprecated.map(Rc::from),
                },
            )
        })
        .collect();
    for (name, number) in [
        ("EXTR_OVERWRITE", 0),
        ("EXTR_SKIP", 1),
        ("EXTR_PREFIX_SAME", 2),
        ("EXTR_PREFIX_ALL", 3),
        ("EXTR_PREFIX_INVALID", 4),
        ("EXTR_PREFIX_IF_EXISTS", 5),
        ("EXTR_IF_EXISTS", 6),
        ("EXTR_REFS", 256),
    ]
    .into_iter()
    .chain(crate::output_buffer::CONSTANTS.iter().copied())
    {
        values.entry(name.into()).or_insert(GlobalConstant {
            value: Value::Int(number),
            deprecated: None,
        });
    }
    values
}

impl ConstantTable {
    pub(crate) fn new(native: Rc<NativeConstants>) -> Self {
        Self {
            native,
            values: HashMap::new(),
            user_order: Vec::new(),
            origins: HashMap::new(),
            resolved: HashMap::new(),
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&GlobalConstant> {
        self.values.get(name).or_else(|| self.native.get(name))
    }

    pub(crate) fn user_names(&self) -> impl Iterator<Item = &str> {
        self.user_order.iter().map(String::as_str)
    }

    pub(crate) fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// A new namespace constant can replace an earlier global fallback.
    pub(crate) fn insert(
        &mut self,
        name: String,
        value: Value,
        origin: Option<ConstantOrigin>,
        deprecated: Option<String>,
    ) {
        self.resolved.clear();
        self.user_order.push(name.clone());
        if let Some(origin) = origin {
            self.origins.insert(name.clone(), origin);
        }
        self.values.insert(
            name,
            GlobalConstant {
                value,
                deprecated: deprecated.map(Rc::from),
            },
        );
    }

    pub(crate) fn origin(&self, name: &str) -> Option<ConstantOrigin> {
        self.origins.get(name).copied()
    }

    /// Failed fetches are never cached: a later define must remain observable.
    pub(crate) fn fetch(
        &mut self,
        unit: Arc<CompiledUnit>,
        symbol: SymbolId,
    ) -> Result<GlobalConstant, NativeError> {
        let site = ConstantSite {
            unit: Arc::as_ptr(&unit) as usize,
            symbol: symbol.0,
        };
        if let Some(resolved) = self.resolved.get(&site) {
            return Ok(GlobalConstant {
                value: resolved.value.clone(),
                deprecated: None,
            });
        }
        let definition = unit
            .symbols
            .get(symbol.index())
            .ok_or_else(|| internal_error!("constant symbol"))?;
        let value = self
            .get(&definition.name)
            .or_else(|| definition.fallback.as_ref().and_then(|name| self.get(name)))
            .cloned()
            .ok_or_else(|| {
                crate::unsupported_items::omitted_item(
                    phera_source::deprecations::ItemKind::Constant,
                    definition.fallback.as_deref().unwrap_or(&definition.name),
                )
                .unwrap_or_else(|| {
                    NativeError::error(format!("Undefined constant \"{}\"", definition.name))
                })
            })?;
        // PHP raises the notice on every read, even at the same bytecode site.
        if value.deprecated.is_none() {
            self.resolved.insert(
                site,
                ResolvedConstant {
                    _unit: unit,
                    value: value.value.clone(),
                },
            );
        }
        Ok(value)
    }
}

/// Constants introduced by `define()` have a file but no declaration attributes.
#[derive(Clone, Copy)]
pub(crate) enum ConstantOrigin {
    Declaration(FunctionIndex),
    Define(FunctionIndex),
}
