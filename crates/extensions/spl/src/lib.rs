//! Standard PHP Library containers and iterator protocols.
//!
//! ```php
//! $items = new ArrayObject(['key' => 'value']);
//! foreach ($items as $key => $value) {}
//! ```

use phera_extension_api::{
    ArrayKey, Context, Extension, NativeError, NativeResult, Object, PhpArray, Registry, Value,
};
use std::rc::Rc;

mod arrays;
mod fixed_array;
mod interfaces;
mod linked_list;
mod object_storage;
mod weak;

pub struct Spl;

impl Extension for Spl {
    fn name(&self) -> &'static str {
        "spl"
    }

    fn register(&self, registry: &mut Registry) {
        interfaces::register(registry);
        arrays::register(registry);
        object_storage::register(registry);
        weak::register(registry);
        linked_list::register(registry);
        fixed_array::register(registry);
    }
}

type This<'a> = Option<&'a Rc<Object>>;

fn receiver(this: This<'_>) -> Result<&Rc<Object>, NativeError> {
    this.ok_or_else(|| NativeError::error("Missing SPL method receiver"))
}

fn store<T: 'static>(object: &Object, value: T) -> Result<(), NativeError> {
    *object.native.try_borrow_mut().map_err(|_| busy())? = Some(Box::new(value));
    Ok(())
}

fn read_state<T: Default + 'static, R>(
    object: &Object,
    read: impl FnOnce(&T) -> R,
) -> Result<R, NativeError> {
    let data = object.native.try_borrow().map_err(|_| busy())?;
    Ok(
        match data.as_ref().and_then(|state| state.downcast_ref::<T>()) {
            Some(state) => read(state),
            None => read(&T::default()),
        },
    )
}

fn update_state<T: Default + 'static, R>(
    object: &Object,
    change: impl FnOnce(&mut T) -> R,
) -> Result<R, NativeError> {
    let mut data = object.native.try_borrow_mut().map_err(|_| busy())?;
    let state = data
        .get_or_insert_with(|| Box::<T>::default())
        .downcast_mut::<T>()
        .ok_or_else(|| NativeError::error("Invalid SPL object state"))?;
    Ok(change(state))
}

fn busy() -> NativeError {
    NativeError::error("Cannot access SPL storage while it is being modified")
}

fn make<T: 'static>(
    context: &mut dyn Context,
    class: &str,
    state: T,
) -> Result<Rc<Object>, NativeError> {
    let Value::Object(object) = context.create_object(class, PhpArray::new())? else {
        return Err(NativeError::error(
            "SPL class factory did not return an object",
        ));
    };
    store(&object, state)?;
    Ok(object)
}

fn list(values: impl IntoIterator<Item = Value>) -> Value {
    Value::from(
        values
            .into_iter()
            .enumerate()
            .map(|(index, value)| (ArrayKey::Int(index as i64), value))
            .collect::<PhpArray>(),
    )
}

fn legacy_serialization(_: &mut dyn Context, _: This<'_>, _: &[Value]) -> NativeResult {
    Err(NativeError::error(
        "SPL legacy serialization is not implemented",
    ))
}
