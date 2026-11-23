//! What a request may change about its classes, kept outside the classes.
//!
//! ```php
//! class Counter {
//!     public static int $count = 0;
//!     const ORIGIN = new Point(0, 0);
//! }
//! Counter::$count++;          // this request's value: the class never changes
//! var_dump(Counter::ORIGIN);  // evaluated once per request, on first use
//! ```
//!
//! A linked class is immutable, so one `Rc<Class>` can serve every copy of
//! a request's state: a snapshot of a booted application and each request
//! forked from it share their classes. What PHP lets a request change, its
//! static properties and the values of constants that are evaluated when
//! first read, lives here instead. The class keeps a slot number; the
//! request keeps the slot.
//!
//! A child class that does not redeclare an inherited static property
//! shares its parent's slot, as PHP shares the property: `Child::$count++`
//! changes `Counter::$count` too.
//!
//! Slots are numbered by the worker (see [`SlotNumbers`]), so that a class
//! linked by one request of the worker has slots that mean the same in
//! every other: a request's tables grow to a slot when it first uses it.

use std::cell::Cell;
use std::rc::Rc;

use phera_value::Value;

use super::class::{ConstantState, PropertyDefault};
use crate::reference_holders::StaticProperty;

/// The number of a static property's storage in [`ClassStates`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StaticSlot(pub(crate) u32);

/// The number of an evaluated constant's state in [`ClassStates`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConstantSlot(u32);

/// The worker's next static and constant slot numbers.
#[derive(Default)]
pub(crate) struct SlotNumbers {
    statics: Cell<u32>,
    constants: Cell<u32>,
}

impl SlotNumbers {
    fn next(counter: &Cell<u32>) -> u32 {
        let slot = counter.get();
        counter.set(slot.saturating_add(1));
        slot
    }
}

/// One request's static properties and evaluated class constants.
pub(crate) struct ClassStates {
    /// Made on first use from the declaration's default value.
    statics: Vec<Option<Rc<StaticProperty>>>,
    constants: Vec<ConstantState>,
    numbers: Rc<SlotNumbers>,
    /// The clones whose `clone with` updates are running, in which each
    /// readonly property may change once. Kept here, where the interpreter
    /// loop already looks, so that its readonly initialization can leave
    /// such a clone to the VM.
    pub(crate) clone_scopes: crate::classes::cloning::CloneScopes,
}

impl Default for ClassStates {
    /// States with a numbering of their own (a command-line run, a test).
    fn default() -> Self {
        Self::for_worker(&Rc::default())
    }
}

impl ClassStates {
    /// The bytes of the slot tables, for a fork's footprint.
    pub(crate) fn heap_bytes(&self) -> usize {
        self.statics.capacity() * std::mem::size_of::<Option<Rc<StaticProperty>>>()
            + self.constants.capacity() * std::mem::size_of::<ConstantState>()
    }

    /// A new static property's slot, for a declaration being linked.
    /// A request's states, with slots numbered by its worker.
    pub(crate) fn for_worker(numbers: &Rc<SlotNumbers>) -> Self {
        ClassStates {
            statics: Vec::new(),
            constants: Vec::new(),
            numbers: numbers.clone(),
            clone_scopes: Default::default(),
        }
    }

    /// The worker's slot numbering these states use.
    pub(crate) fn numbers(&self) -> Rc<SlotNumbers> {
        self.numbers.clone()
    }

    pub(crate) fn new_static(&mut self) -> StaticSlot {
        StaticSlot(SlotNumbers::next(&self.numbers.statics))
    }

    /// A new evaluated constant's slot, for a declaration being linked.
    pub(crate) fn new_constant(&mut self) -> ConstantSlot {
        ConstantSlot(SlotNumbers::next(&self.numbers.constants))
    }

    /// The storage of a static property, made from its default on first use.
    /// `None` for an instance property or a slot this request does not have.
    pub(crate) fn static_property(
        &mut self,
        property: &PropertyDefault,
    ) -> Option<Rc<StaticProperty>> {
        let StaticSlot(slot) = property.static_storage?;
        let slot = slot as usize;
        if self.statics.len() <= slot {
            self.statics.resize(slot + 1, None);
        }
        let entry = &mut self.statics[slot];
        Some(
            entry
                .get_or_insert_with(|| {
                    StaticProperty::new(
                        property.value.clone(),
                        property.initializer.is_none(),
                        property.initialized,
                    )
                })
                .clone(),
        )
    }

    /// The storage of a static property, if this request made it already.
    pub(crate) fn existing_static(
        &self,
        property: &PropertyDefault,
    ) -> Option<&Rc<StaticProperty>> {
        let StaticSlot(slot) = property.static_storage?;
        self.statics.get(slot as usize)?.as_ref()
    }

    /// The storage in `slot`, if this request made it already: a static
    /// property read site's (see `crate::static_sites`).
    pub(crate) fn existing_slot(
        &self,
        StaticSlot(slot): StaticSlot,
    ) -> Option<&Rc<StaticProperty>> {
        self.statics.get(slot as usize)?.as_ref()
    }

    /// The state of an evaluated constant.
    pub(crate) fn constant(&self, ConstantSlot(slot): ConstantSlot) -> &ConstantState {
        self.constants
            .get(slot as usize)
            .unwrap_or(&ConstantState::Uninitialized)
    }

    pub(crate) fn set_constant(&mut self, ConstantSlot(slot): ConstantSlot, state: ConstantState) {
        let slot = slot as usize;
        if self.constants.len() <= slot {
            self.constants
                .resize_with(slot + 1, || ConstantState::Uninitialized);
        }
        self.constants[slot] = state;
    }
}

/// A fork gets copies of the statics made so far and of evaluated values.
impl crate::snapshot::ForkCopy for ClassStates {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        let statics = self
            .statics
            .iter()
            .map(|property| {
                property.as_ref().map(|property| {
                    let value = Value::Reference(property.reference.borrow().clone());
                    let Value::Reference(cell) = fork.value(&value) else {
                        return property.clone();
                    };
                    StaticProperty::forked(cell, property)
                })
            })
            .collect();
        let constants = self
            .constants
            .iter()
            .map(|state| match state {
                ConstantState::Ready(value) => ConstantState::Ready(fork.value(value)),
                ConstantState::Evaluating | ConstantState::Uninitialized => {
                    ConstantState::Uninitialized
                }
            })
            .collect();
        // No clone is being made while a snapshot is forked.
        ClassStates {
            statics,
            constants,
            numbers: self.numbers.clone(),
            clone_scopes: Default::default(),
        }
    }
}
