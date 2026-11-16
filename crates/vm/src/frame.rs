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
    /// Present only for methods; ordinary functions never inherit $this.
    pub(crate) method: Option<Box<crate::classes::MethodContext>>,
    /// Resolved callees are shared by all frames executing this unit.
    pub(crate) cache: crate::SymbolCache,
    /// Argument expressions may nest calls, so unfinished calls form a stack.
    pub(crate) calls: Vec<crate::call_state::PendingCall>,
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
    /// Saved argument view for PHP func_get_args and related intrinsics.
    pub(crate) arguments: Vec<Value>,
    /// Optional names for variables accessed through dynamic scope.
    pub(crate) dynamic: crate::dynamic_variables::DynamicVariables,
    /// Compiler-assigned slots hold active foreach traversals.
    pub(crate) iterators: crate::iterators::Iterators,
}

impl Frame {
    /// Allocate the function window with undefined slots and no Rust call recursion.
    pub(crate) fn new(
        cache: crate::SymbolCache,
        registers: &mut crate::registers::Registers,
        unit: Arc<CompiledUnit>,
        function_id: Option<usize>,
        destination: Option<Reg>,
    ) -> Self {
        let function = function_id
            .and_then(|i| unit.functions.get(i))
            .unwrap_or(&unit.main);
        let count = function.registers;
        let dynamic =
            crate::dynamic_variables::DynamicVariables::new(function.dynamic_scope.into());
        let iterators = crate::iterators::Iterators::new(function.iterators);
        let base = registers.len();
        registers.extend_to(base + count as usize);
        Self {
            method: None,
            cache,
            calls: vec![],
            unit,
            function_id,
            ip: 0,
            destination,
            base,
            count: count as usize,
            arguments: vec![],
            dynamic,

            iterators,
        }
    }

    #[inline]
    pub fn slots<'a>(&self, registers: &'a crate::registers::Registers) -> &'a [Option<Value>] {
        registers.window(self.base, self.count)
    }

    #[inline]
    pub fn slots_mut<'a>(
        &self,
        registers: &'a mut crate::registers::Registers,
    ) -> &'a mut [Option<Value>] {
        registers.window_mut(self.base, self.count)
    }

    pub fn function(&self) -> &Function {
        self.function_id
            .and_then(|i| self.unit.functions.get(i))
            .unwrap_or(&self.unit.main)
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
    pub fn get(&self, registers: &crate::registers::Registers, register: Reg) -> Value {
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
        registers: &mut crate::registers::Registers,
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
