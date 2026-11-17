//! Register windows and write-through PHP reference assignment.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// Undefined named locals warn and read as null; assigned slots dereference aliases.
    pub(crate) fn read_local(&mut self, register: Reg) -> Value {
        if let Some(value) = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(register.index()))
            .and_then(Option::as_ref)
            .map(|value| value.deref().clone())
        {
            return value;
        }
        if let Some(name) = self
            .current()
            .and_then(|frame| {
                frame
                    .function()
                    .locals
                    .get(register.index())
                    .and_then(|name| frame.unit.names.get(name.index()))
            })
            .cloned()
        {
            self.warn(format!("Undefined variable ${name}"));
        }
        Value::Null
    }

    /// Only replacing a whole subject restarts foreach; array copy-on-write keeps its cursor.
    pub(crate) fn restart_subject_iterators(&mut self, register: Reg) {
        let Some(frame) = self.call_stack.current_mut() else {
            return;
        };
        let slots = self.registers.window(frame.base, frame.count);
        if let Some(Some(subject)) = slots.get(register.index()) {
            frame.iterators.restart_for(subject, slots);
        }
    }

    /// Assignment updates the shared cell when a local already holds a PHP reference.
    #[inline]
    pub(crate) fn assign_local(&mut self, register: Reg, value: Value) -> Result<(), NativeError> {
        if let Some(reference) = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(register.index()))
            .and_then(Option::as_ref)
            .and_then(|value| match value {
                Value::Reference(reference) => Some(reference.clone()),
                _ => None,
            })
        {
            let value = self.check_reference_assignment(&reference, value)?;
            *reference
                .try_borrow_mut()
                .map_err(|_| internal_error!("borrowed reference"))? = value;
        } else {
            self.set(register, value);
        }
        Ok(())
    }
}

/// One contiguous value stack supplies disjoint windows to PHP heap frames.
/// An empty slot is an undefined variable; PHP null is a present Value::Null.
pub(crate) struct Registers {
    slots: Vec<Option<Value>>,
}

impl Registers {
    pub(crate) fn new() -> Self {
        Self { slots: Vec::new() }
    }

    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    /// Allocate undefined locals without constructing PHP null values.
    pub(crate) fn extend_to(&mut self, end: usize) {
        self.slots.resize_with(end, || None);
    }

    pub(crate) fn push_undefined(&mut self) {
        self.slots.push(None);
    }

    /// Returning releases the callee's window while preserving caller aliases.
    pub(crate) fn truncate(&mut self, base: usize) {
        self.slots.truncate(base);
    }

    #[inline]
    pub(crate) fn window(&self, base: usize, count: usize) -> &[Option<Value>] {
        self.slots.get(base..base + count).unwrap_or(&[])
    }

    #[inline]
    pub(crate) fn window_mut(&mut self, base: usize, count: usize) -> &mut [Option<Value>] {
        self.slots.get_mut(base..base + count).unwrap_or(&mut [])
    }

    /// The scalar loop keeps the current frame's tail borrowed between slow instructions.
    #[inline]
    pub(crate) fn tail_mut(&mut self, base: usize) -> &mut [Option<Value>] {
        &mut self.slots[base..]
    }
}
