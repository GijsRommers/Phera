//! `$store[$key]` uses native state and returns a reference for indirect writes.

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, Context, MethodEntry, NativeDimensions, NativeError,
    NativeResult, Object, PhpArray, Registry, Value,
};

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        interfaces: &["ArrayAccess", "Countable"],
        methods: const { &[MethodEntry::public("__construct", &[], construct)] },
        hooks: const {
            Some(&ClassHooks {
                dimensions: Some(NativeDimensions {
                    get,
                    exists,
                    set,
                    unset,
                }),
                count: Some(count),
                ..ClassHooks::EMPTY
            })
        },
        ..ClassEntry::class("EngineStore")
    });
}

fn missing() -> NativeError {
    NativeError::error("Native store state unavailable")
}

fn construct(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let object = this.ok_or_else(missing)?;
    *object.native.try_borrow_mut().map_err(|_| missing())? = Some(Box::new(PhpArray::new()));
    Ok(Value::Null)
}

fn with_store<T>(
    object: &Object,
    apply: impl FnOnce(&mut PhpArray) -> Result<T, NativeError>,
) -> Result<T, NativeError> {
    let mut storage = object.native.try_borrow_mut().map_err(|_| missing())?;
    let store = storage
        .as_mut()
        .and_then(|data| data.downcast_mut::<PhpArray>())
        .ok_or_else(missing)?;
    apply(store)
}

fn get(context: &mut dyn Context, object: &Object, offset: Value) -> NativeResult {
    let key = ArrayKey::from_value_diag(&offset, context.diagnostics())?;
    with_store(object, |store| {
        let Some(value) = store.get_mut(&key) else {
            return Ok(Value::Null);
        };
        if !value.is_reference() {
            *value = Value::Reference(Rc::new(RefCell::new(std::mem::take(value))));
        }
        Ok(value.clone())
    })
}

fn exists(context: &mut dyn Context, object: &Object, offset: Value) -> Result<bool, NativeError> {
    let key = ArrayKey::from_value_diag(&offset, context.diagnostics())?;
    with_store(object, |store| {
        Ok(store
            .get(&key)
            .is_some_and(|value| !value.deref().is_null()))
    })
}

fn set(
    context: &mut dyn Context,
    object: &Object,
    offset: Value,
    value: Value,
) -> Result<(), NativeError> {
    let key = (!offset.is_null())
        .then(|| ArrayKey::from_value_diag(&offset, context.diagnostics()))
        .transpose()?;
    with_store(object, |store| {
        if let Some(key) = key {
            if let Some(Value::Reference(reference)) = store.get(&key) {
                *reference.try_borrow_mut().map_err(|_| missing())? = value;
            } else {
                store.insert(key, value);
            }
        } else {
            store.push(value)?;
        }
        Ok(())
    })
}

fn unset(context: &mut dyn Context, object: &Object, offset: Value) -> Result<(), NativeError> {
    let key = ArrayKey::from_value_diag(&offset, context.diagnostics())?;
    with_store(object, |store| {
        store.remove(&key);
        Ok(())
    })
}

fn count(_: &mut dyn Context, object: &Object) -> Result<i64, NativeError> {
    with_store(object, |store| Ok(store.len() as i64))
}
