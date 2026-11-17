//! Heap call frames and conversion of immutable bytecode constants.

use std::{rc::Rc, sync::Arc};

use phera_bytecode::{CompiledUnit, Constant, Function, Reg};
use phera_value::{PhpArray, Value};

use crate::AccessPath;

/// Foreach either owns a value snapshot or follows a live reference path.
pub(crate) struct IteratorState {
    /// By-value foreach keeps this owner to separate subsequent array writes.
    pub(crate) snapshot: Option<Rc<PhpArray>>,
    /// By-reference foreach resolves this place again as the array changes.
    pub(crate) source: Option<AccessPath>,
    /// The array storage position, distinct from a PHP integer key.
    pub(crate) position: Option<usize>,
    /// Distinguishes an untouched iterator from an exhausted one.
    pub(crate) started: bool,
}

/// A PHP activation lives on the heap; calls never recurse through the Rust stack.
pub(crate) struct Frame {
    /// Active callable objects stay alive; lexical closures also own their static storage.
    pub(crate) closure: Option<Rc<phera_value::Object>>,
    /// Present only for methods; ordinary functions never inherit $this.
    pub(crate) method: Option<crate::classes::MethodContext>,
    /// Resolved callees are shared by all frames executing this unit.
    pub(crate) cache: crate::SymbolCache,
    /// Preparation depth on entry, used to release abandoned argument windows.
    pub(crate) call_start: usize,
    pub(crate) unit: Arc<CompiledUnit>,
    /// None selects the unit top level, including include/eval frames.
    pub(crate) function_id: Option<usize>,
    /// Instruction pointer: the next instruction, published before observable operations.
    pub(crate) ip: usize,
    /// The caller register receiving the result; absent at a callback boundary.
    pub(crate) destination: Option<Reg>,
    /// Start of this frame window in the request register stack.
    pub(crate) base: usize,
    /// Includes dynamic locals introduced after frame construction.
    pub(crate) count: usize,
    /// PHP traces omit defaulted parameters beyond the supplied call arity.
    pub(crate) supplied_arguments: usize,
    /// Saved argument view for PHP func_get_args and related intrinsics.
    pub(crate) saved_arguments: Option<usize>,
    /// Number of supplied positional arguments, excluding named variadic entries.
    pub(crate) argument_count: usize,
    /// Surplus values are stored immediately past the declared register window.
    pub(crate) extra_arguments: usize,
    /// Optional names for variables accessed through dynamic scope.
    pub(crate) dynamic: crate::dynamic_variables::DynamicVariables,
    /// Compiler-assigned slots hold active foreach traversals.
    pub(crate) iterators: crate::iterators::Iterators,
}

impl Frame {
    /// Allocate the function window with undefined slots and no Rust call recursion.
    pub(crate) fn new(
        cache: crate::SymbolCache,
        registers: &mut crate::calls::registers::Registers,
        unit: Arc<CompiledUnit>,
        function_id: Option<usize>,
        destination: Option<Reg>,
        base: usize,
    ) -> Self {
        let function = function_id
            .and_then(|i| unit.functions.get(i))
            .unwrap_or(&unit.main);
        let count = function.registers;
        let dynamic =
            crate::dynamic_variables::DynamicVariables::new(function.dynamic_scope.into());
        let iterators = crate::iterators::Iterators::new(function.iterators);
        registers.grow_to(base + count as usize);
        Self {
            closure: None,
            method: None,
            cache,
            call_start: 0,
            unit,
            function_id,
            ip: 0,
            destination,
            base,
            count: count as usize,
            saved_arguments: None,
            argument_count: 0,
            extra_arguments: 0,
            supplied_arguments: 0,
            dynamic,
            iterators,
        }
    }

    /// Local operands address only this activation, excluding saved arguments.
    #[inline]
    pub fn slots<'a>(
        &self,
        registers: &'a crate::calls::registers::Registers,
    ) -> &'a [Option<Value>] {
        registers.window(self.base, self.count)
    }

    /// Raw local writes must stay inside the declared or dynamically grown window.
    #[inline]
    pub fn slots_mut<'a>(
        &self,
        registers: &'a mut crate::calls::registers::Registers,
    ) -> &'a mut [Option<Value>] {
        registers.window_mut(self.base, self.count)
    }

    /// Top-level and function activations share immutable bytecode access.
    pub fn function(&self) -> &Function {
        self.function_id
            .and_then(|i| self.unit.functions.get(i))
            .unwrap_or(&self.unit.main)
    }

    /// PHP exposes current fixed parameters, but keeps original variadic arguments separately.
    pub(crate) fn argument(
        &self,
        registers: &crate::calls::registers::Registers,
        index: usize,
    ) -> Value {
        let function = self.function();
        let fixed = function.parameters.len()
            - usize::from(function.argument_layout == phera_bytecode::ArgumentLayout::Variadic);
        let slot = if index < fixed {
            self.base + index
        } else if let Some(base) = self.saved_arguments {
            base + index
        } else {
            self.base + self.count + index - fixed
        };
        registers
            .window(slot, 1)
            .first()
            .and_then(Option::as_ref)
            .map(|value| value.deref().clone())
            .unwrap_or_default()
    }

    /// IP has already advanced, so diagnostics use the previous instruction line.
    pub(crate) fn line(&self) -> usize {
        if self.ip == 0 && self.function_id.is_some() {
            return self.function().definition_line as usize;
        }
        self.function()
            .lines
            .get(self.ip.saturating_sub(1))
            .copied()
            .unwrap_or(1) as usize
    }

    /// Ordinary reads see through PHP references without copying the reference cell.
    #[inline]
    pub fn get(&self, registers: &crate::calls::registers::Registers, register: Reg) -> Value {
        self.slots(registers)
            .get(register.0 as usize)
            .and_then(Option::as_ref)
            .map(|value| value.deref().clone())
            .unwrap_or_default()
    }

    /// Store a raw slot value; PHP write-through assignment is handled separately.
    #[inline]
    pub fn set(
        &mut self,
        registers: &mut crate::calls::registers::Registers,
        register: Reg,
        value: Value,
    ) {
        if let Some(slot) = self.slots_mut(registers).get_mut(register.0 as usize) {
            *slot = Some(value)
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

impl Frame {
    /// The unit-owned slot is keyed by this function and its published next IP.
    pub(crate) fn site_cache(&self) -> Option<&std::cell::RefCell<crate::unit_caches::CachedSite>> {
        self.cache.site(self.function_id, self.ip)
    }
}
