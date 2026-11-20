//! Dimension reads and writes use a native handle unless getHash is overridden.
//!
//! ```php
//! isset($storage[$object]); // true even when the stored value is null
//! ```

use std::rc::Rc;

use phera_extension_api::{
    Context, LocalFuture, NativeDimensions, NativeError, NativeResult, Object, Value,
};

use super::{CLASS, Key, attach, detach, hash, read};
use crate::receiver;

pub(super) const HANDLERS: NativeDimensions = NativeDimensions {
    direct: Some(phera_extension_api::DirectDimensions {
        read: |storage, offset| {
            let Value::Object(object) = offset else {
                return None;
            };
            lookup(storage, &Key::Handle(object.id)).ok()
        },
        write: |storage, offset, info| {
            let Value::Object(object) = offset else {
                return false;
            };
            attach(
                storage,
                Key::Handle(object.id),
                object.clone(),
                info.deref().clone(),
            )
            .is_ok()
        },
    }),
    reference: None,
    bind: None,
    fallback_if_overridden: &["gethash"],
    get: |_, storage, value| lookup(storage, &Key::Handle(object(value, "offsetGet")?.id)),
    exists: |_, storage, value| contains(storage, &Key::Handle(object(value, "offsetExists")?.id)),
    set: |_, storage, value, info| {
        let object = object(value, "offsetSet")?;
        attach(storage, Key::Handle(object.id), object, info)
    },
    unset: |_, storage, value| {
        detach(
            storage,
            &Key::Handle(object(value, "offsetUnset")?.id),
            false,
        )
    },
};

fn object(value: Value, method: &str) -> Result<Rc<Object>, NativeError> {
    let value = value.deref();
    match &*value {
        Value::Object(object) => Ok(object.clone()),
        value => {
            let given = match value {
                Value::Bool(value) => value.to_string(),
                value => value.type_name(),
            };
            Err(NativeError::type_error(format!(
                "{CLASS}::{method}(): Argument #1 ($object) must be of type object, {given} given"
            )))
        }
    }
}

fn lookup(storage: &Object, key: &Key) -> NativeResult {
    read(storage, |data| {
        data.entries.get(key).map(|e| e.info.deref().clone())
    })?
    .ok_or_else(|| NativeError::throw("UnexpectedValueException", "Object not found"))
}

fn contains(storage: &Object, key: &Key) -> Result<bool, NativeError> {
    read(storage, |data| data.entries.contains_key(key))
}

#[derive(Clone, Copy)]
enum Operation {
    Set,
    Get,
    Unset,
    Exists,
}

fn invoke<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    operation: Operation,
    method: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let storage = receiver(this.as_ref())?;
        let mut arguments = arguments.into_iter();
        let object = object(arguments.next().unwrap_or(Value::Null), method)?;
        let key = hash(context, storage, &object).await?;
        match operation {
            Operation::Get => lookup(storage, &key),
            Operation::Exists => contains(storage, &key).map(Value::Bool),
            Operation::Set => {
                attach(
                    storage,
                    key,
                    object,
                    arguments.next().unwrap_or(Value::Null),
                )?;
                Ok(Value::Null)
            }
            Operation::Unset => {
                detach(storage, &key, true)?;
                Ok(Value::Null)
            }
        }
    })
}

pub(super) fn offset_set(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke(context, this, args, Operation::Set, "offsetSet")
}

pub(super) fn offset_get(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke(context, this, args, Operation::Get, "offsetGet")
}

pub(super) fn offset_exists(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke(context, this, args, Operation::Exists, "offsetExists")
}

pub(super) fn offset_unset(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke(context, this, args, Operation::Unset, "offsetUnset")
}

pub(super) fn attach_old(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke(context, this, args, Operation::Set, "attach")
}

pub(super) fn detach_old(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke(context, this, args, Operation::Unset, "detach")
}

pub(super) fn contains_old(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke(context, this, args, Operation::Exists, "contains")
}
