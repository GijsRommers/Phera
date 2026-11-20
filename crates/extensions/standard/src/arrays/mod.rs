//! Array functions, and the helpers several of them share: building a list,
//! copying entries with or without their integer keys, and changing the
//! array behind a by-reference parameter.

use std::rc::Rc;

use phera_extension_api::{ArrayKey, NativeError, PhpArray, Registry, Value, args};

mod build;
mod callback;
mod callback_sets;
mod callback_sort;
mod combine;
mod combine_recursive;
mod filter;
mod find;
mod keys;
mod multisort;
mod ordering;
mod pointer;
mod range;
mod range_endpoint;
mod range_values;
mod sets;
mod slice;
mod sort;

/// Registered implementations shared with the SPL array containers.
pub fn associative_sort(name: &str) -> Option<phera_extension_api::FunctionEntry> {
    sort::FUNCTIONS
        .iter()
        .chain(callback_sort::FUNCTIONS)
        .find(|entry| entry.name == name)
        .copied()
}
mod stack;
mod tables;
mod walk;
pub(crate) mod zend_sort;

pub(crate) fn register(registry: &mut Registry) {
    multisort::register(registry);
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
/// The VM passes a by-reference parameter as a reference and has checked
/// that it holds an array. Should a caller ever break that, or should the
/// variable already be borrowed, the script gets an `Error`. Clone entries
/// that may refer back to the array and resolve them after this call returns.
pub(super) fn change_array_argument<T>(
    arguments: &[Value],
    change: impl FnOnce(&mut PhpArray) -> Result<T, NativeError>,
) -> Result<T, NativeError> {
    let variable = args::reference(arguments, 0).ok_or_else(not_a_variable)?;
    let mut value = variable.try_borrow_mut().map_err(|_| variable_in_use())?;
    match &mut *value {
        Value::Array(array) => change(Rc::make_mut(array)),
        _ => Err(not_a_variable()),
    }
}

/// The by-reference array parameter did not arrive as a variable holding an
/// array, which the VM rules out before a native function runs.
pub(super) fn not_a_variable() -> NativeError {
    NativeError::error("Internal error: a by-reference array argument is not an array variable")
}

/// The variable is being read or written further up the same call.
pub(super) fn variable_in_use() -> NativeError {
    NativeError::error("Internal error: the array variable is already in use")
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
