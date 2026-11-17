//! Register windows and write-through PHP reference assignment.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// The value of a local variable, or `None` when it was never assigned.
    /// A PHP reference is followed to the value it points at.
    pub(crate) fn read_defined_local(&self, register: Reg) -> Option<Value> {
        self.current()
            .and_then(|frame| frame.slots(&self.registers).get(register.index()))
            .and_then(Option::as_ref)
            .map(|value| value.deref().clone())
    }

    /// Like `read_defined_local`, but an unassigned variable warns
    /// ("Undefined variable $name") and reads as null, as in PHP.
    pub(crate) fn read_local(&mut self, register: Reg) -> Value {
        if let Some(value) = self.read_defined_local(register) {
            return value;
        }
        if let Some(name) = self
            .current()
            .and_then(|frame| {
                frame
                    .function(&self.code)
                    .locals
                    .get(register.index())
                    .and_then(|name| frame.unit(&self.code).names.get(name.index()))
            })
            .cloned()
        {
            self.warn(format!("Undefined variable ${name}"));
        }
        Value::Null
    }

    /// Only replacing a whole subject restarts foreach; array copy-on-write keeps its cursor.
    pub(crate) fn restart_subject_iterators(&mut self, register: Reg) {
        let Some(frame) = self.call_stack.current().copied() else {
            return;
        };
        let slots = self.registers.window(frame.base, frame.count);
        if let Some(Some(subject)) = slots.get(register.index()) {
            self.call_stack.extras[frame.depth]
                .iterators
                .restart_for(subject, slots);
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
    /// Each request owns a single contiguous value stack.
    pub(crate) fn new() -> Self {
        Self { slots: Vec::new() }
    }

    /// Window reservations begin above every live slot.
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    /// Grow a window with undefined slots without shrinking another live window.
    pub(crate) fn grow_to(&mut self, end: usize) {
        if self.slots.len() < end {
            self.slots.resize_with(end, || None);
        }
    }

    /// Dynamic variables grow before reserved windows; their bases move with the insertion.
    pub(crate) fn insert_undefined(&mut self, index: usize) {
        self.slots.insert(index, None);
    }

    /// Included dynamic variables extend the current window.
    pub(crate) fn push_undefined(&mut self) {
        self.slots.push(None);
    }

    /// Returning releases the callee's window while preserving caller aliases.
    pub(crate) fn truncate(&mut self, base: usize) {
        self.slots.truncate(base);
    }

    /// Bounded slices keep callers separate from unfinished callees.
    #[inline]
    pub(crate) fn window(&self, base: usize, count: usize) -> &[Option<Value>] {
        self.slots.get(base..base + count).unwrap_or(&[])
    }

    /// Mutations are restricted to a single reserved region.
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
