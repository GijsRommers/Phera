//! Request-owned code turns a PHP activation into an index.
//!
//! `f(g(1))` keeps both functions in this table; entering either function copies
//! its FunctionIndex instead of creating another unit owner.

use phera_bytecode::{CompiledUnit, Function};
use std::sync::Arc;

/// A stable request-local address for top-level code or a function body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FunctionIndex(pub(crate) u32);

/// Relative class keywords select a frame scope; all other symbols have permanent links.
#[derive(Clone, Copy)]
pub(crate) enum ClassSymbol {
    Named(crate::names::Name),
    Lexical,
    Called,
    Parent,
}

/// Immutable code and request-local literal data are retained once per loaded unit.
pub(crate) struct LoadedUnit {
    pub(crate) unit: Arc<CompiledUnit>,
    pub(crate) data: crate::unit_data::UnitData,
    first: FunctionIndex,
    pub(crate) calls: std::cell::RefCell<Vec<Option<crate::calls::call_state::Callee>>>,
    pub(crate) names: Vec<crate::names::Name>,
    pub(crate) symbols: Vec<crate::names::Name>,
    pub(crate) class_symbols: Vec<ClassSymbol>,
    pub(crate) class_links:
        std::cell::RefCell<Vec<Option<crate::classes::class_table::ClassIndex>>>,
}

/// Every body has one entry even when it has not been declared as a PHP callable yet.
pub(crate) struct FunctionEntry {
    pub(crate) body: Function,
    pub(crate) required: usize,
    pub(crate) untyped_values: bool,
    pub(crate) return_mask: Option<phera_bytecode::TypeMask>,
    pub(crate) signature: crate::signatures::SignatureIndex,
    pub(crate) unit: usize,
    pub(crate) local: Option<usize>,
}

/// Loaded code lives until the request ends, independently of activation lifetimes.
#[derive(Default)]
pub(crate) struct CodeTable {
    pub(crate) units: Vec<LoadedUnit>,
    pub(crate) functions: Vec<FunctionEntry>,
}

impl CodeTable {
    /// Loading is cold; calls within the loaded unit use the resulting numeric address.
    pub(crate) fn load(
        &mut self,
        unit: &Arc<CompiledUnit>,
        signatures: &mut crate::signatures::Signatures,
        classes: &mut crate::classes::ClassTable,
    ) -> FunctionIndex {
        if let Some(loaded) = self
            .units
            .iter()
            .find(|loaded| Arc::ptr_eq(&loaded.unit, unit))
        {
            return loaded.first;
        }
        let first = FunctionIndex(self.functions.len() as u32);
        let owner = self.units.len();
        for (local, body) in std::iter::once((None, &unit.main)).chain(
            unit.functions
                .iter()
                .enumerate()
                .map(|(index, body)| (Some(index), body)),
        ) {
            let parameters = body
                .parameters
                .iter()
                .map(|parameter| crate::calls::call_state::ParameterSpec::user(unit, parameter))
                .collect();
            let signature = signatures.insert(body.name.clone(), parameters);
            self.functions.push(FunctionEntry {
                required: body
                    .parameters
                    .iter()
                    .rposition(|parameter| !parameter.has_default && !parameter.variadic)
                    .map_or(0, |index| index + 1),
                untyped_values: body
                    .parameters
                    .iter()
                    .all(|parameter| parameter.ty.is_none() && !parameter.by_reference),
                return_mask: body.return_type.map(|ty| unit.types[ty.index()].mask),
                body: body.clone(),
                signature,
                unit: owner,
                local,
            });
        }
        let names = unit
            .names
            .iter()
            .map(|name| classes.names.intern(name))
            .collect();
        let symbols = unit
            .symbols
            .iter()
            .map(|symbol| classes.names.intern(&symbol.name))
            .collect();
        let class_symbols = unit
            .symbols
            .iter()
            .map(|symbol| match symbol.written.as_str() {
                "self" => ClassSymbol::Lexical,
                "static" => ClassSymbol::Called,
                "parent" => ClassSymbol::Parent,
                _ => ClassSymbol::Named(classes.names.intern(&symbol.name)),
            })
            .collect();
        self.units.push(LoadedUnit {
            class_symbols,
            class_links: std::cell::RefCell::new(vec![None; unit.symbols.len()]),
            calls: std::cell::RefCell::new(vec![None; unit.symbols.len()]),
            names,
            symbols,
            unit: unit.clone(),
            data: crate::unit_data::UnitData::new(unit),
            first,
        });
        first
    }

    /// Resolve a unit-local body while loading or handling a dynamic callable.
    pub(crate) fn index(&self, unit: &Arc<CompiledUnit>, local: Option<usize>) -> FunctionIndex {
        let loaded = self
            .units
            .iter()
            .find(|loaded| Arc::ptr_eq(&loaded.unit, unit))
            .expect("unit loaded before linking its functions");
        FunctionIndex(loaded.first.0 + local.map_or(0, |local| local as u32 + 1))
    }

    /// Instructions are borrowed from their single retained owner.
    pub(crate) fn function(&self, index: FunctionIndex) -> &Function {
        &self.functions[index.0 as usize].body
    }

    /// Constant and name tables belong to the loaded unit rather than the activation.
    pub(crate) fn loaded(&self, index: FunctionIndex) -> &LoadedUnit {
        &self.units[self.functions[index.0 as usize].unit]
    }
}

impl crate::Vm {
    /// Includes load between synchronous batches, when the VM is the only code-table owner.
    pub(crate) fn load_unit(&mut self, unit: &Arc<CompiledUnit>) {
        std::rc::Rc::get_mut(&mut self.code)
            .expect("no executing batch during unit loading")
            .load(unit, &mut self.signatures, &mut self.classes);
    }
}

#[cfg(test)]
mod tests {
    /// Request-owned code survives all frames without a frame retaining another Arc.
    #[test]
    fn cached_units_stay_alive() {
        let unit = Arc::new(phera_compiler::compile("<?php", "cache.php").unwrap());
        let weak = Arc::downgrade(&unit);
        let mut vm = crate::Vm::new(Arc::new(phera_extension_api::Registry::new()));
        vm.load_unit(&unit);
        drop(unit);
        assert!(weak.upgrade().is_some());
    }
    use std::sync::Arc;
}
