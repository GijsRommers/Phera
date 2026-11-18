//! Storage that lives as long as one request.

use std::any::{Any, TypeId};
use std::collections::HashMap;

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
    /// One value per type. `TypeId` is Rust's run-time name for a type.
    slots: HashMap<TypeId, Box<dyn Any>>,
}

impl RequestState {
    /// The state of type `T`, created on first use.
    pub fn get_or_default<T: Default + 'static>(&mut self) -> &mut T {
        let slot = self
            .slots
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(T::default()));
        // The slot under `T`'s id always holds a `T`. If that were ever not
        // so, start over with a fresh value instead of panicking.
        if !slot.is::<T>() {
            *slot = Box::new(T::default());
        }
        match slot.downcast_mut::<T>() {
            Some(state) => state,
            None => unreachable!("the slot was just checked to hold this type"),
        }
    }

    /// The state of type `T`, if anything created it.
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.slots.get(&TypeId::of::<T>())?.downcast_ref()
    }

    /// Removes and returns the state of type `T`. The runtime uses this after
    /// a request to collect what an extension gathered.
    pub fn take<T: 'static>(&mut self) -> Option<T> {
        let slot = self.slots.remove(&TypeId::of::<T>())?;
        slot.downcast().ok().map(|state| *state)
    }
}

/// Runtime configuration for `http_build_query(['a' => 1, 'b' => 2])`.
pub struct QuerySeparator(pub phera_value::PhpString);

impl Default for QuerySeparator {
    fn default() -> Self {
        Self("&".into())
    }
}
