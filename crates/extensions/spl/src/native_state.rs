//! SPL objects can own other native objects without recursing when they are freed.
//!
//! ```php
//! $value = 0;
//! for ($i = 0; $i < 100000; $i++) $value = new ArrayObject([$value]);
//! unset($value);
//! ```

use std::{any::Any, cell::RefCell};

use phera_extension_api::value::Fork;
use phera_extension_api::{NativeError, Object, Value};

use crate::busy;

struct Stored<T: 'static>(Option<T>);

thread_local! {
    static DROPPING: RefCell<Option<Vec<Box<dyn Any>>>> = const { RefCell::new(None) };
}

impl<T: 'static> Drop for Stored<T> {
    fn drop(&mut self) {
        let mut value = self.0.take();
        let outermost = DROPPING.try_with(|pending| {
            let mut pending = pending.borrow_mut();
            if let Some(queue) = pending.as_mut() {
                if let Some(value) = value.take() {
                    queue.push(Box::new(value));
                }
                false
            } else {
                *pending = Some(Vec::new());
                true
            }
        });
        if !matches!(outermost, Ok(true)) {
            return;
        }

        drop(value);
        loop {
            let next = DROPPING.with(|pending| {
                let mut pending = pending.borrow_mut();
                let next = pending.as_mut().and_then(Vec::pop);
                if next.is_none() {
                    *pending = None;
                }
                next
            });
            let Some(next) = next else { break };
            drop(next);
        }
    }
}

pub(crate) fn store<T: 'static>(object: &Object, value: T) -> Result<(), NativeError> {
    *object.native.try_borrow_mut().map_err(|_| busy())? = Some(Box::new(Stored(Some(value))));
    Ok(())
}

pub(crate) fn read_state<T: Default + 'static, R>(
    object: &Object,
    read: impl FnOnce(&T) -> R,
) -> Result<R, NativeError> {
    let data = object.native.try_borrow().map_err(|_| busy())?;
    Ok(
        match data
            .as_ref()
            .and_then(|state| state.downcast_ref::<Stored<T>>())
            .and_then(|state| state.0.as_ref())
        {
            Some(state) => read(state),
            None => read(&T::default()),
        },
    )
}

pub(crate) fn update_state<T: Default + 'static, R>(
    object: &Object,
    change: impl FnOnce(&mut T) -> R,
) -> Result<R, NativeError> {
    let mut data = object.native.try_borrow_mut().map_err(|_| busy())?;
    let state = data
        .get_or_insert_with(|| Box::new(Stored(Some(T::default()))))
        .downcast_mut::<Stored<T>>()
        .and_then(|state| state.0.as_mut())
        .ok_or_else(|| NativeError::error("Invalid SPL object state"))?;
    Ok(change(state))
}

pub(crate) fn update_present_state<T: 'static, R>(
    object: &Object,
    change: impl FnOnce(&mut T) -> R,
) -> Result<Option<R>, NativeError> {
    let mut data = object.native.try_borrow_mut().map_err(|_| busy())?;
    let Some(stored) = data.as_deref_mut() else {
        return Ok(None);
    };
    let state = stored
        .downcast_mut::<Stored<T>>()
        .and_then(|state| state.0.as_mut())
        .ok_or_else(|| NativeError::error("Invalid SPL object state"))?;
    Ok(Some(change(state)))
}

/// How a fork of a snapshot copies one kind of SPL state: a clone, with
/// every PHP value inside replaced by the fork's copy.
///
/// ```php
/// $queue = new SplQueue;
/// $queue->enqueue(new Job);
/// Phera\snapshot();   // each request's queue holds its own Job
/// ```
pub(crate) trait ForkValues: Clone + 'static {
    /// Replaces the values of this clone by the fork's; an error says why
    /// the state cannot be copied (an iteration in progress).
    fn fork_values(&mut self, fork: &mut Fork<'_>) -> Result<(), String>;
}

/// The fork of a state of type `T`, if `state` is one.
pub(crate) type StoredFork = fn(&dyn Any, &mut Fork<'_>) -> Option<Result<Box<dyn Any>, String>>;

pub(crate) fn fork_as<T: ForkValues>(
    state: &dyn Any,
    fork: &mut Fork<'_>,
) -> Option<Result<Box<dyn Any>, String>> {
    let stored = state.downcast_ref::<Stored<T>>()?;
    let mut copy = stored.0.clone();
    if let Some(copy) = &mut copy
        && let Err(reason) = copy.fork_values(fork)
    {
        return Some(Err(reason));
    }
    Some(Ok(Box::new(Stored(copy))))
}

/// Every SPL state kind a fork can copy.
const FORKS: &[StoredFork] = &[
    crate::arrays::FORK,
    crate::linked_list::FORK,
    crate::heap::FORK,
    crate::fixed_array::FORK,
    crate::object_storage::FORK,
    crate::multiple_iterator::FORK,
    crate::weak::FORK,
    crate::weak::REFERENCE_FORK,
];

/// The fork hook of every SPL class with native state.
pub(crate) fn fork_spl(
    _object: &Object,
    state: &dyn Any,
    fork: &mut Fork<'_>,
) -> Result<Box<dyn Any>, String> {
    FORKS
        .iter()
        .find_map(|fork_state| fork_state(state, fork))
        .unwrap_or_else(|| Err("it is iterating, or its state cannot be copied".into()))
}

/// Maps a value in place.
pub(crate) fn fork_value(value: &mut Value, fork: &mut Fork<'_>) {
    *value = fork.value(value);
}
