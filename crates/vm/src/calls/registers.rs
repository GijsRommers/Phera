//! Register windows and write-through PHP reference assignment.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;

impl Vm {
    /// An undefined CV supplies null. The warning handler can still change
    /// the other operand before the operation uses it.
    pub(crate) async fn warn_missing_operand(
        &mut self,
        register: Reg,
    ) -> Result<bool, NativeError> {
        if self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(register.index()))
            .is_some_and(Option::is_some)
        {
            return Ok(false);
        }
        self.read_local(register);
        self.drain_error_handlers().await?;
        Ok(true)
    }

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
                    .function()
                    .locals
                    .get(register.index())
                    .and_then(|name| frame.unit().names.get(name.index()))
            })
            .cloned()
        {
            self.warn(format!("Undefined variable ${name}"));
        }
        Value::Null
    }

    /// Only replacing a whole subject restarts foreach; array copy-on-write keeps its cursor.
    pub(crate) fn restart_subject_iterators(&mut self, register: Reg) {
        let Some(frame) = self.current() else {
            return;
        };
        if !frame.frame.has_extras() {
            return;
        }
        let slots = self.registers.window(frame.base(), frame.count());
        if let Some(Some(subject)) = slots.get(register.index()) {
            let iterators = &mut self.call_stack.current_extras_mut().iterators;
            iterators.restart_for(subject, slots);
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

/// The register stack of a request: every frame's registers, one after the
/// other, with the arguments of calls being prepared on top.
///
/// An empty slot is an undefined variable; PHP null is a present
/// `Value::Null`. Every slot at or above `top` is empty, so a new frame's
/// registers need no clearing: returning empties them instead, and the
/// vector keeps its length.
pub(crate) struct Registers {
    slots: Vec<Option<Value>>,
    top: usize,
}

impl Registers {
    /// Each request owns a single contiguous value stack.
    pub(crate) fn new() -> Self {
        Self {
            slots: Vec::new(),
            top: 0,
        }
    }

    /// The first slot that no frame or prepared call uses.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.top
    }

    /// Make the slots below `end` part of the stack. They are empty already.
    #[inline]
    pub(crate) fn grow_to(&mut self, end: usize) {
        if self.top < end {
            if self.slots.len() < end {
                self.slots.resize_with(end + 64, || None);
            }
            self.top = end;
        }
    }

    /// Dynamic variables grow before reserved windows; their bases move with the insertion.
    pub(crate) fn insert_undefined(&mut self, index: usize) {
        self.slots.insert(index, None);
        self.top += 1;
    }

    /// Included dynamic variables extend the current window.
    pub(crate) fn push_undefined(&mut self) {
        self.grow_to(self.top + 1);
    }

    /// Empty every slot from `base` up, releasing what they held: a frame
    /// or a prepared call leaves the stack.
    #[inline(always)]
    pub(crate) fn truncate(&mut self, base: usize) {
        if base < self.top {
            for slot in &mut self.slots[base..self.top] {
                *slot = None;
            }
            self.top = base;
        }
    }

    /// Empty the registers of a frame that ends, from `start` (its window,
    /// with `$this` or the closure below `base`) up. PHP releases a
    /// function's variables first, in order, and `$this` after them, so a
    /// destructor of a local runs before the destructor of `$this`.
    #[cold]
    #[inline(never)]
    pub(crate) fn release_frame(&mut self, start: usize, base: usize) {
        let prefix = [start, start + 1].map(|slot| {
            if slot < base {
                self.slots.get_mut(slot).and_then(Option::take)
            } else {
                None
            }
        });
        self.truncate(start);
        drop(prefix);
    }

    /// Whether a slot from `start` up to `base` holds the only reference to
    /// an object: `$this` or the closure of a frame that is the last to
    /// hold it.
    #[inline]
    pub(crate) fn holds_last_reference(&self, start: usize, base: usize) -> bool {
        self.slots
            .get(start..base)
            .unwrap_or_default()
            .iter()
            .any(|slot| matches!(slot, Some(Value::Object(object)) if std::rc::Rc::strong_count(object) == 1))
    }

    /// Move the active tail into a generator without retaining the stack's capacity.
    pub(crate) fn detach(
        &mut self,
        start: usize,
        registers: &mut Vec<Option<Value>>,
    ) -> Result<(), NativeError> {
        let window = self
            .slots
            .get_mut(start..self.top)
            .ok_or_else(|| internal_error!("generator register window"))?;
        registers.extend(window.iter_mut().map(Option::take));
        if registers.capacity() > 2 * registers.len().max(1) {
            registers.shrink_to_fit();
        }
        self.top = start;
        Ok(())
    }

    /// Restore a generator's registers above the caller's prepared calls.
    pub(crate) fn attach(&mut self, slots: &mut Vec<Option<Value>>) {
        let start = self.top;
        self.grow_to(start + slots.len());
        for (slot, value) in self.slots[start..self.top].iter_mut().zip(slots.drain(..)) {
            *slot = value;
        }
    }

    /// Move an argument into an empty slot of a newly reserved frame.
    #[inline(always)]
    pub(crate) fn move_slot(&mut self, from: usize, to: usize) {
        self.slots.swap(from, to);
    }

    /// Take the value out of a slot, leaving it empty.
    #[inline(always)]
    pub(crate) fn take(&mut self, index: usize) -> Value {
        self.slots[index].take().unwrap_or_default()
    }

    /// Store a value in a slot, dropping what it held.
    #[inline(always)]
    pub(crate) fn put(&mut self, index: usize, value: Value) {
        self.slots[index] = Some(value);
    }

    /// Bounded slices keep callers separate from unfinished callees.
    #[inline(always)]
    pub(crate) fn window(&self, base: usize, count: usize) -> &[Option<Value>] {
        self.slots.get(base..base + count).unwrap_or(&[])
    }

    /// Mutations are restricted to a single reserved region.
    #[inline(always)]
    pub(crate) fn window_mut(&mut self, base: usize, count: usize) -> &mut [Option<Value>] {
        self.slots.get_mut(base..base + count).unwrap_or(&mut [])
    }

    /// The scalar loop keeps the current frame's tail borrowed between slow instructions.
    #[inline(always)]
    pub(crate) fn tail_mut(&mut self, base: usize) -> &mut [Option<Value>] {
        &mut self.slots[base..]
    }
}
