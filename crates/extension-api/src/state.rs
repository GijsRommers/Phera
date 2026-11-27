//! Storage that lives as long as one request.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// What extensions remember during a request: the response headers, the
/// output buffers, an open session.
///
/// Each extension defines its own state type and reaches it through that
/// type, so extensions cannot touch each other's state and the VM needs to
/// know none of them:
///
/// ```
/// use phera_extension_api::RequestState;
///
/// #[derive(Default)]
/// struct Counter(u32);
///
/// let mut state = RequestState::default();
/// state.get_or_default::<Counter>().0 += 1;
/// assert_eq!(state.get::<Counter>().map(|counter| counter.0), Some(1));
/// ```
#[derive(Default)]
pub struct RequestState {
    /// One value per type in a flat vector (typically 4–12 entries per request).
    /// Linear scan over `TypeId` avoids SipHash overhead and hash table allocations.
    slots: Vec<(TypeId, Slot)>,
}

/// One extension's state, with the name of its type for error messages.
struct Slot {
    value: Box<dyn Any>,
    name: &'static str,
}

/// How a fork of a snapshot gets an extension's state; registered with
/// [`crate::Registry::fork_state`]. `None` means the fork starts without
/// it, as a new request does ([`fresh_state`]); [`copy_state`] copies it.
pub type StateFork = fn(&dyn Any, &mut phera_value::Fork<'_>) -> Option<Box<dyn Any>>;

/// The fork starts without this state, as a new request would.
pub fn fresh_state(_state: &dyn Any, _fork: &mut phera_value::Fork<'_>) -> Option<Box<dyn Any>> {
    None
}

/// The fork gets a clone of this state; for state that holds no PHP value.
pub fn copy_state<T: Clone + 'static>(
    state: &dyn Any,
    _fork: &mut phera_value::Fork<'_>,
) -> Option<Box<dyn Any>> {
    Some(Box::new(state.downcast_ref::<T>()?.clone()))
}

impl RequestState {
    /// The state of type `T`, created on first use.
    #[inline]
    pub fn get_or_default<T: Default + 'static>(&mut self) -> &mut T {
        let id = TypeId::of::<T>();
        let index = match self.slots.iter().position(|(key, _)| *key == id) {
            Some(index) => index,
            None => {
                let index = self.slots.len();
                self.slots.push((
                    id,
                    Slot {
                        value: Box::new(T::default()),
                        name: std::any::type_name::<T>(),
                    },
                ));
                index
            }
        };
        let slot = &mut self.slots[index].1;
        // The slot under `T`'s id always holds a `T`. If that were ever not
        // so, start over with a fresh value instead of panicking.
        if !slot.value.is::<T>() {
            slot.value = Box::new(T::default());
        }
        match slot.value.downcast_mut::<T>() {
            Some(state) => state,
            None => unreachable!("the slot was just checked to hold this type"),
        }
    }

    /// The state of type `T`, if anything created it.
    #[inline]
    pub fn get<T: 'static>(&self) -> Option<&T> {
        let id = TypeId::of::<T>();
        self.slots
            .iter()
            .find(|(key, _)| *key == id)?
            .1
            .value
            .downcast_ref()
    }

    /// Removes and returns the state of type `T`. The runtime uses this after
    /// a request to collect what an extension gathered.
    pub fn take<T: 'static>(&mut self) -> Option<T> {
        let id = TypeId::of::<T>();
        let index = self.slots.iter().position(|(key, _)| *key == id)?;
        let (_, slot) = self.slots.swap_remove(index);
        slot.value.downcast().ok().map(|state| *state)
    }

    /// The state of a fork of a snapshot: each extension's state as its
    /// registered [`StateFork`] makes it. A type with no registered fork
    /// cannot be copied safely; its name is the error.
    pub fn fork(
        &self,
        forks: &HashMap<TypeId, StateFork>,
        fork: &mut phera_value::Fork<'_>,
    ) -> Result<RequestState, &'static str> {
        let mut copy = RequestState {
            slots: Vec::with_capacity(self.slots.len()),
        };
        for (type_id, slot) in &self.slots {
            let make = forks.get(type_id).ok_or(slot.name)?;
            if let Some(value) = make(slot.value.as_ref(), fork) {
                copy.slots.push((
                    *type_id,
                    Slot {
                        value,
                        name: slot.name,
                    },
                ));
            }
        }
        Ok(copy)
    }
}

/// Child resources can check this flag at teardown after a PHP execution timeout.
#[derive(Clone, Default)]
pub struct ProcessCancellation(Arc<AtomicBool>);

impl ProcessCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Stored in [`RequestState`] when a request runs from a strict AOT image that
/// forbids runtime code evaluation or dynamic class aliasing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageSeal {
    pub forbid_eval: bool,
    pub forbid_class_alias: bool,
}
