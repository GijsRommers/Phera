use phera_bytecode::{CompiledUnit, Constant, Function, Place, Reg};
use phera_value::{PhpArray, Value};
use std::{collections::HashMap, rc::Rc, sync::Arc};
pub struct IteratorState {
    pub snapshot: Option<Rc<PhpArray>>,
    pub source: Option<Place>,
    pub position: Option<usize>,
    pub started: bool,
}
pub struct Frame {
    pub argument_diagnostics: HashMap<u16, Vec<Vec<u8>>>,
    pub unit: Arc<CompiledUnit>,
    pub function_id: Option<usize>,
    pub ip: usize,
    pub destination: Option<Reg>,
    pub slots: Vec<Option<Value>>,
    pub arguments: Vec<Value>,
    pub dynamic: HashMap<String, Reg>,
    pub iterators: HashMap<u16, IteratorState>,
}
impl Frame {
    pub fn new(
        unit: Arc<CompiledUnit>,
        function_id: Option<usize>,
        destination: Option<Reg>,
    ) -> Self {
        let count = function_id
            .and_then(|i| unit.functions.get(i))
            .unwrap_or(&unit.main)
            .registers;
        Self {
            unit,
            function_id,
            ip: 0,
            destination,
            slots: vec![None; count],
            arguments: vec![],
            argument_diagnostics: HashMap::new(),
            dynamic: HashMap::new(),
            iterators: HashMap::new(),
        }
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
            .unwrap_or(1)
    }
    pub fn get(&self, r: Reg) -> Value {
        self.slots
            .get(r.0 as usize)
            .and_then(Option::as_ref)
            .map(|v| v.deref().clone())
            .unwrap_or_default()
    }
    pub fn set(&mut self, r: Reg, v: Value) {
        if let Some(s) = self.slots.get_mut(r.0 as usize) {
            *s = Some(v)
        }
    }
}
pub fn value(c: &Constant) -> Value {
    match c {
        Constant::Null => Value::Null,
        Constant::EmptyArray => Value::Array(Rc::new(PhpArray::new())),
        Constant::Bool(v) => Value::Bool(*v),
        Constant::Int(v) => Value::Int(*v),
        Constant::Float(v) => Value::Float(*v),
        Constant::Bytes(v) => Value::from(v.clone()),
    }
}
