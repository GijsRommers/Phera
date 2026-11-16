//! Heap call frames and conversion of immutable bytecode constants.
use crate::AccessPath;
use phera_bytecode::{CompiledUnit, Constant, Function, Reg};
use phera_value::{PhpArray, Value};
use std::{rc::Rc, sync::Arc};
pub struct IteratorState {
    pub snapshot: Option<Rc<PhpArray>>,
    pub source: Option<AccessPath>,
    pub position: Option<usize>,
    pub started: bool,
}
pub struct Frame {
    pub cache: crate::SymbolCache,
    pub calls: Vec<crate::pending::PendingCall>,
    pub unit: Arc<CompiledUnit>,
    pub function_id: Option<usize>,
    pub ip: usize,
    pub destination: Option<Reg>,
    pub base: usize,
    pub count: usize,
    pub arguments: Vec<Value>,
    pub dynamic: crate::dynamic_variables::DynamicVariables,
    pub iterators: crate::iterators::Iterators,
}
impl Frame {
    pub fn new(
        cache: crate::SymbolCache,
        registers: &mut Vec<Option<Value>>,
        unit: Arc<CompiledUnit>,
        function_id: Option<usize>,
        destination: Option<Reg>,
    ) -> Self {
        let function = function_id
            .and_then(|i| unit.functions.get(i))
            .unwrap_or(&unit.main);
        let count = function.registers;
        let dynamic = crate::dynamic_variables::DynamicVariables::new(function.dynamic_scope);
        let iterators = crate::iterators::Iterators::new(function.iterators);
        let base = registers.len();
        registers.resize_with(base + count as usize, || None);
        Self {
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
    pub fn slots<'a>(&self, registers: &'a [Option<Value>]) -> &'a [Option<Value>] {
        registers
            .get(self.base..self.base + self.count)
            .unwrap_or(&[])
    }
    #[inline]
    pub fn slots_mut<'a>(&self, registers: &'a mut [Option<Value>]) -> &'a mut [Option<Value>] {
        registers
            .get_mut(self.base..self.base + self.count)
            .unwrap_or(&mut [])
    }
    pub fn function(&self) -> &Function {
        self.function_id
            .and_then(|i| self.unit.functions.get(i))
            .unwrap_or(&self.unit.main)
    }
    pub fn line(&self) -> usize {
        self.function()
            .lines
            .get(self.ip.saturating_sub(1))
            .copied()
            .unwrap_or(1) as usize
    }
    #[inline]
    pub fn get(&self, registers: &[Option<Value>], r: Reg) -> Value {
        self.slots(registers)
            .get(r.0 as usize)
            .and_then(Option::as_ref)
            .map(|v| v.deref().clone())
            .unwrap_or_default()
    }
    #[inline]
    pub fn set(&mut self, registers: &mut [Option<Value>], r: Reg, v: Value) {
        if let Some(s) = self.slots_mut(registers).get_mut(r.0 as usize) {
            *s = Some(v)
        }
    }
}
pub fn value(c: &Constant) -> Value {
    match c {
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
        Constant::Bool(v) => Value::Bool(*v),
        Constant::Int(v) => Value::Int(*v),
        Constant::Float(v) => Value::Float(*v),
        Constant::String(v) => Value::from(v.clone()),
    }
}
