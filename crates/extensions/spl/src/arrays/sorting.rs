//! SPL sorts preserve keys and use the same comparisons as PHP's array sort methods.
//!
//! ```php
//! $a = new ArrayObject(['b' => 2, 'a' => 1]);
//! $a->asort();
//! ```

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{Context, LocalFuture, NativeError, NativeFn, NativeResult, Value};

use super::state::{self, Target};
use crate::receiver;

fn replace(
    object: &phera_extension_api::Object,
    array: &RefCell<Value>,
) -> Result<(), phera_extension_api::NativeError> {
    if let Value::Array(array) = &*array.try_borrow().map_err(|_| crate::busy())? {
        Target::of(object)?.update(|target| {
            *target = array.as_ref().clone();
            Ok(())
        })?;
    }
    Ok(())
}

fn sort<'a>(
    context: &'a mut dyn Context,
    object: Option<Rc<phera_extension_api::Object>>,
    mut args: Vec<Value>,
    name: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let object = receiver(object.as_ref())?;
        if matches!(name, "uasort" | "uksort") {
            let callable = super::dimensions::arg(&args, 0);
            context.validate_callback(name, 2, "callback", &callable)?;
        }
        state::check_mutable(object)?;
        let array = Rc::new(RefCell::new(Value::Array(Target::of(object)?.copy()?)));
        let handler = phera_ext_standard::associative_sort(name)
            .and_then(|entry| match entry.handler {
                NativeFn::Async(handler) => Some(handler),
                _ => None,
            })
            .ok_or_else(|| NativeError::error("Missing SPL sort function"))?;
        args.insert(0, Value::Reference(array.clone()));
        state::update(object, |state| {
            state.sorting = true;
            Ok(())
        })?;
        let result = handler(context, args).await;
        state::update(object, |state| {
            state.sorting = false;
            Ok(())
        })?;
        replace(object, &array)?;
        result
    })
}

pub(super) fn asort(
    context: &mut dyn Context,
    this: Option<Rc<phera_extension_api::Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    sort(context, this, args, "asort")
}

pub(super) fn ksort(
    context: &mut dyn Context,
    this: Option<Rc<phera_extension_api::Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    sort(context, this, args, "ksort")
}

pub(super) fn natsort(
    context: &mut dyn Context,
    this: Option<Rc<phera_extension_api::Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    sort(context, this, args, "natsort")
}

pub(super) fn natcasesort(
    context: &mut dyn Context,
    this: Option<Rc<phera_extension_api::Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    sort(context, this, args, "natcasesort")
}

pub(super) fn uasort(
    context: &mut dyn Context,
    this: Option<Rc<phera_extension_api::Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    sort(context, this, args, "uasort")
}

pub(super) fn uksort(
    context: &mut dyn Context,
    this: Option<Rc<phera_extension_api::Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    sort(context, this, args, "uksort")
}
