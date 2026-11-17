//! Array functions, and the helpers several of them share: building a list,
//! copying entries with or without their integer keys, and changing the
//! array behind a by-reference parameter.

use std::rc::Rc;

use phera_extension_api::{ArrayKey, NativeError, PhpArray, Registry, Value, args};

mod build;
mod callback;
mod callback_set_merge;
mod callback_sets;
mod callback_sort;
mod combine;
mod combine_recursive;
mod filter;
mod find;
mod keys;
mod ordering;
mod pointer;
mod range;
mod range_endpoint;
mod range_values;
mod sets;
mod slice;
mod sort;
mod stack;
mod tables;
mod walk;
pub(crate) mod zend_sort;

pub(crate) fn register(registry: &mut Registry) {
    let modules = [
        build::FUNCTIONS,
        callback_sets::FUNCTIONS,
        callback_sort::FUNCTIONS,
        combine::FUNCTIONS,
        filter::FUNCTIONS,
        find::FUNCTIONS,
        keys::FUNCTIONS,
        pointer::FUNCTIONS,
        range::FUNCTIONS,
        sets::FUNCTIONS,
        slice::FUNCTIONS,
        sort::FUNCTIONS,
        stack::FUNCTIONS,
        tables::FUNCTIONS,
        walk::FUNCTIONS,
    ];
    for entry in modules.into_iter().flatten() {
        registry.function(*entry);
    }
}

/// What happens to integer keys when entries are copied into a new array;
/// string keys are always kept.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum IntegerKeys {
    /// Numbered again from 0, as `array_slice` does by default.
    Renumber,
    Preserve,
}

impl IntegerKeys {
    /// From a `$preserve_keys` argument.
    fn preserve_if(preserve: bool) -> Self {
        if preserve {
            Self::Preserve
        } else {
            Self::Renumber
        }
    }
}

/// Appends one entry to `result`, keeping or renumbering its key.
pub(super) fn append_entry(
    result: &mut PhpArray,
    key: ArrayKey,
    value: &Value,
    keys: IntegerKeys,
) -> Result<(), NativeError> {
    if keys == IntegerKeys::Preserve || matches!(key, ArrayKey::String(_)) {
        result.insert(key, value.clone());
    } else {
        result.push(value.clone())?;
    }
    Ok(())
}

/// Runs `change` on the array behind the by-reference parameter 0, as
/// `sort($array)` and `array_push($array, …)` need.
///
/// `prepare_arguments` passes a by-reference parameter as a reference and
/// has checked that it holds an array; nothing else can borrow the variable
/// while a synchronous native function runs.
pub(super) fn change_array_argument<T>(
    arguments: &[Value],
    change: impl FnOnce(&mut PhpArray) -> T,
) -> T {
    let variable = args::reference(arguments, 0)
        .expect("prepare_arguments passes by-reference parameters as references");
    let mut value = variable.borrow_mut();
    let Value::Array(array) = &mut *value else {
        unreachable!("prepare_arguments checked that the variable holds an array");
    };
    change(Rc::make_mut(array))
}

/// A PHP list, `[0 => $a, 1 => $b, …]`, of these values.
pub(crate) fn list(values: impl IntoIterator<Item = Value>) -> PhpArray {
    let mut array = PhpArray::new();
    for value in values {
        // Only an array whose next key would pass PHP_INT_MAX refuses a push.
        array
            .push(value)
            .expect("a new array has a free key for every value");
    }
    array
}
