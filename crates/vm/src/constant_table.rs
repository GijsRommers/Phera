//! Resolve constant symbols once, invalidating fallbacks when PHP defines names.

use crate::code_table::FunctionIndex;
use std::{cell::OnceCell, rc::Rc, sync::Arc};

use foldhash::{HashMap, HashMapExt};
use phera_bytecode::{CompiledUnit, SymbolId};
use phera_extension_api::{NativeError, Registry};
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
    values: Rc<HashMap<String, GlobalConstant>>,
    user_order: Rc<Vec<String>>,
    origins: Rc<HashMap<String, ConstantOrigin>>,
    resolved: HashMap<ConstantSite, ResolvedConstant>,
}

pub(crate) struct NativeConstants {
    registry: Arc<Registry>,
    values: OnceCell<HashMap<String, GlobalConstant>>,
}

impl NativeConstants {
    pub(crate) fn new(registry: Arc<Registry>) -> Self {
        Self {
            registry,
            values: OnceCell::new(),
        }
    }

    fn get(&self, name: &str) -> Option<&GlobalConstant> {
        self.values
            .get_or_init(|| native_constants(&self.registry))
            .get(name)
    }

    #[cfg(test)]
    pub(crate) fn is_initialized(&self) -> bool {
        self.values.get().is_some()
    }
}

fn native_constants(registry: &Registry) -> HashMap<String, GlobalConstant> {
    let mut values: HashMap<_, _> = registry
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
    pub(crate) fn native_definitions(&self) -> Rc<NativeConstants> {
        self.native.clone()
    }

    pub(crate) fn new(native: Rc<NativeConstants>) -> Self {
        Self {
            native,
            values: Rc::new(HashMap::new()),
            user_order: Rc::new(Vec::new()),
            origins: Rc::new(HashMap::new()),
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
        Rc::make_mut(&mut self.user_order).push(name.clone());
        if let Some(origin) = origin {
            Rc::make_mut(&mut self.origins).insert(name.clone(), origin);
        }
        Rc::make_mut(&mut self.values).insert(
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

impl crate::snapshot::ForkCopy for GlobalConstant {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        GlobalConstant {
            value: fork.value(&self.value),
            deprecated: self.deprecated.clone(),
        }
    }
}

/// A constant can hold an object (an enum case); the fork's is its copy.
/// When every user constant is trivially shareable (scalars, strings, pure
/// arrays), share the whole table copy-on-write in O(1).
impl crate::snapshot::ForkCopy for ConstantTable {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        let values = if self
            .values
            .values()
            .all(|constant| fork.is_trivially_shared(&constant.value))
        {
            self.values.clone()
        } else {
            Rc::new(self.values.fork_copy(fork))
        };
        ConstantTable {
            native: self.native.clone(),
            values,
            user_order: self.user_order.clone(),
            origins: self.origins.clone(),
            resolved: Default::default(),
        }
    }
}
