//! Compact activations keep owners and optional PHP features out of the stack.
//!
//! During `f(g(1))`, the pending f window stays below g's active registers:
//!
//! ```text
//! frames:      [main: code 0, base 0] [g: code 2, base 12]
//! registers:   [main locals] [f arguments] [g arguments and locals]
//! side tables: depth 0 -> dynamic names; depth 1 -> empty
//! code table:  0 -> main, 1 -> f, 2 -> g (retained for the request)
//! ```
//!
//! Popping g copies only numbers. Its optional side storage is released separately.

use crate::code_table::{CodeTable, FunctionIndex};
use phera_bytecode::{Constant, Function, Reg};
use phera_value::{PhpArray, Value};
use std::rc::Rc;

/// Foreach either owns a value snapshot or follows a live reference path.
pub(crate) struct IteratorState {
    pub(crate) snapshot: Option<Rc<PhpArray>>,
    pub(crate) source: Option<crate::AccessPath>,
    pub(crate) position: Option<usize>,
    pub(crate) started: bool,
}

/// The hot activation contains no allocation and no reference-counted owner.
#[derive(Clone, Copy)]
pub(crate) struct Frame {
    pub(crate) function: FunctionIndex,
    pub(crate) ip: usize,
    pub(crate) destination: Option<Reg>,
    pub(crate) base: usize,
    pub(crate) count: usize,
    pub(crate) call_start: usize,
    pub(crate) depth: usize,
}

impl Frame {
    /// Local operands exclude saved argument views in the tail of the register stack.
    #[inline]
    pub(crate) fn slots<'a>(
        &self,
        registers: &'a crate::calls::registers::Registers,
    ) -> &'a [Option<Value>] {
        registers.window(self.base, self.count)
    }

    /// A write may only address this activation's declared or dynamic local window.
    #[inline]
    pub(crate) fn slots_mut<'a>(
        &self,
        registers: &'a mut crate::calls::registers::Registers,
    ) -> &'a mut [Option<Value>] {
        registers.window_mut(self.base, self.count)
    }

    /// Code metadata is borrowed without cloning its owner.
    pub(crate) fn function<'a>(&self, code: &'a CodeTable) -> &'a Function {
        code.function(self.function)
    }

    /// Unit names and constants remain in request-owned storage.
    pub(crate) fn unit<'a>(
        &self,
        code: &'a CodeTable,
    ) -> &'a std::sync::Arc<phera_bytecode::CompiledUnit> {
        &code.loaded(self.function).unit
    }

    /// Transitional access to property and type metadata outside this redesign.
    pub(crate) fn cache<'a>(&self, code: &'a CodeTable) -> &'a crate::unit_data::UnitData {
        &code.loaded(self.function).data
    }

    /// Top-level include code has no unit-local function number.
    pub(crate) fn function_id(&self, code: &CodeTable) -> Option<usize> {
        code.functions[self.function.0 as usize].local
    }

    /// IP is already advanced when PHP observes a diagnostic location.
    pub(crate) fn line(&self, code: &CodeTable) -> usize {
        let function = self.function(code);
        if self.ip == 0 && self.function_id(code).is_some() {
            return function.definition_line as usize;
        }
        function
            .lines
            .get(self.ip.saturating_sub(1))
            .copied()
            .unwrap_or(1) as usize
    }

    /// Ordinary reads see through PHP reference cells.
    #[inline]
    pub(crate) fn get(
        &self,
        registers: &crate::calls::registers::Registers,
        register: Reg,
    ) -> Value {
        self.slots(registers)
            .get(register.index())
            .and_then(Option::as_ref)
            .map(|value| value.deref().clone())
            .unwrap_or_default()
    }

    /// Raw writes leave PHP alias handling to the assignment handler.
    #[inline]
    pub(crate) fn set(
        &self,
        registers: &mut crate::calls::registers::Registers,
        register: Reg,
        value: Value,
    ) {
        if let Some(slot) = self.slots_mut(registers).get_mut(register.index()) {
            *slot = Some(value);
        }
    }

    /// Property sites remain until object slots replace property lookup.
    pub(crate) fn site_cache<'a>(
        &self,
        code: &'a CodeTable,
    ) -> Option<&'a std::cell::RefCell<crate::unit_data::CachedSite>> {
        self.cache(code).site(self.function_id(code), self.ip)
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
