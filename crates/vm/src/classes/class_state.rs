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

use std::rc::Rc;

use phera_value::Value;

use super::class::{ConstantState, PropertyDefault};
use crate::reference_holders::StaticProperty;

/// The number of a static property's storage in [`ClassStates`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StaticSlot(u32);

/// The number of an evaluated constant's state in [`ClassStates`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConstantSlot(u32);

/// One request's static properties and evaluated class constants.
#[derive(Default)]
pub(crate) struct ClassStates {
    /// Made on first use from the declaration's default value.
    statics: Vec<Option<Rc<StaticProperty>>>,
    constants: Vec<ConstantState>,
}

impl ClassStates {
    /// A new static property's slot, for a declaration being linked.
    pub(crate) fn new_static(&mut self) -> StaticSlot {
        self.statics.push(None);
        StaticSlot(self.statics.len() as u32 - 1)
    }

    /// A new evaluated constant's slot, for a declaration being linked.
    pub(crate) fn new_constant(&mut self) -> ConstantSlot {
        self.constants.push(ConstantState::Uninitialized);
        ConstantSlot(self.constants.len() as u32 - 1)
    }

    /// The storage of a static property, made from its default on first use.
    /// `None` for an instance property or a slot this request does not have.
    pub(crate) fn static_property(
        &mut self,
        property: &PropertyDefault,
    ) -> Option<Rc<StaticProperty>> {
        let StaticSlot(slot) = property.static_storage?;
        let entry = self.statics.get_mut(slot as usize)?;
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

    /// The state of an evaluated constant.
    pub(crate) fn constant(&self, ConstantSlot(slot): ConstantSlot) -> &ConstantState {
        self.constants
            .get(slot as usize)
            .unwrap_or(&ConstantState::Uninitialized)
    }

    pub(crate) fn set_constant(&mut self, ConstantSlot(slot): ConstantSlot, state: ConstantState) {
        if let Some(entry) = self.constants.get_mut(slot as usize) {
            *entry = state;
        }
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
        ClassStates { statics, constants }
    }
}
