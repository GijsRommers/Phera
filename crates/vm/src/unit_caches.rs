//! Request-local values made once per loaded file: names, literals, call
//! links and the per-instruction site caches.

use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::Rc,
};

use phera_bytecode::{CompiledUnit, Constant};
use phera_value::{PhpArray, Value};

/// Materialized immutable literals share PHP COW values across frame activations.
/// Keyed by constant/symbol index in a retained unit; populated at first unit use.
/// Bytecode literals never mutate, and VM-local ownership prevents cross-request reuse.
pub(crate) struct UnitCache {
    /// Throwable properties and trace entries share the unit's immutable filename.
    trace_file: OnceCell<phera_value::PhpString>,
    pub(crate) names: Vec<phera_value::PhpString>,
    /// What each function symbol of the file was linked to, once its first call
    /// resolved it.
    pub(crate) symbols: RefCell<Vec<Option<crate::calls::pending::LinkedFunction>>>,
    pub(crate) direct_natives: OnceCell<Box<[Cell<Option<crate::calls::sync_native::NativeLink>>]>>,
    pub(crate) constants: Vec<phera_value::Value>,
    pub(crate) class_names: Vec<phera_value::Value>,
    pub(crate) classes: Vec<RefCell<Option<Rc<crate::classes::Class>>>>,
    /// Keyed by immutable unit TypeId and made at first unit use; spelling cannot
    /// change, so scope classification needs no invalidation. Relative types resolve per call.
    pub(crate) type_scopes: Vec<crate::types::DeclarationScope>,
    pub(crate) class_types: Vec<RefCell<Option<ClassTypeEntry>>>,
    main_sites: Vec<RefCell<CachedSite>>,
    function_sites: Vec<Vec<RefCell<CachedSite>>>,
    pub(crate) closure_signatures: RefCell<Vec<Option<crate::calls::call_state::Callee>>>,
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

    /// The site of the instruction of `body` (function `function` of the
    /// file, or its top-level code) that ends at `ip`: the VM looks it up
    /// after the instruction pointer has advanced. `None` for an
    /// instruction without one.
    pub(crate) fn site(
        &self,
        body: &phera_bytecode::Function,
        function: Option<usize>,
        ip: usize,
    ) -> Option<&RefCell<CachedSite>> {
        let slot = body.sites.site(&body.instructions, ip)?;
        match function {
            Some(function) => self.function_sites.get(function)?.get(slot),
            None => self.main_sites.get(slot),
        }
    }
}

/// An empty cell for each instruction that has a site (see
/// `phera_bytecode::SiteTable`); what a site resolves to is allocated when
/// it is first used.
fn instruction_sites(function: &phera_bytecode::Function) -> Vec<RefCell<CachedSite>> {
    (0..function.sites.count(&function.instructions))
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
            type_scopes: unit
                .types
                .iter()
                .map(crate::types::declaration_scope)
                .collect(),
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
