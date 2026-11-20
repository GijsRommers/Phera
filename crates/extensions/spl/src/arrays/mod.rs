//! ArrayObject creates live ArrayIterator views of its backing storage.
//!
//! ```php
//! $items = new ArrayObject([1]);
//! $items->getIterator()->append(2);
//! var_dump($items->getArrayCopy()); // [1, 2]
//! ```

mod cursor;
mod dimensions;
mod methods;
mod properties;
mod registration;
mod serialization;
mod sorting;
mod state;

pub(crate) use registration::register;
pub(crate) use serialization::{legacy_check_mutable, legacy_set_storage, legacy_uses_properties};

const OBJECT: &str = "ArrayObject";
const ITERATOR: &str = "ArrayIterator";
const RECURSIVE: &str = "RecursiveArrayIterator";

/// Creates the ArrayIterator used as AppendIterator's live list of sources.
pub(crate) fn empty_iterator(
    context: &mut dyn phera_extension_api::Context,
) -> Result<std::rc::Rc<phera_extension_api::Object>, phera_extension_api::NativeError> {
    crate::make(context, ITERATOR, state::State::default())
}

pub(crate) fn backing(
    object: &phera_extension_api::Object,
) -> Result<std::rc::Rc<phera_extension_api::PhpArray>, phera_extension_api::NativeError> {
    state::Target::of(object)?.copy()
}

pub(crate) fn push(
    context: &mut dyn phera_extension_api::Context,
    object: &phera_extension_api::Object,
    value: phera_extension_api::Value,
) -> Result<(), phera_extension_api::NativeError> {
    dimensions::set(context, object, phera_extension_api::Value::Null, value)
}
