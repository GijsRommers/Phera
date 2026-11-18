//! SPL objects can own other native objects without recursing when they are freed.
//!
//! ```php
//! $value = 0;
//! for ($i = 0; $i < 100000; $i++) $value = new ArrayObject([$value]);
//! unset($value);
//! ```

use std::{any::Any, cell::RefCell};

use phera_extension_api::{NativeError, Object};

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
