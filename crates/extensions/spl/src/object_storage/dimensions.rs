//! Dimension reads and writes use a native handle unless getHash is overridden.
//!
//! ```php
//! isset($storage[$object]); // true even when the stored value is null
//! ```

use std::rc::Rc;

use phera_extension_api::{
    Context, LocalFuture, NativeDimensions, NativeError, NativeResult, Object, Value,
};

use super::{CLASS, Key, attach, detach, hash, hash_scope::require_mutation_allowed, read};
use crate::receiver;

pub(super) const fn handlers<const CHECK: bool>() -> NativeDimensions {
    NativeDimensions {
        direct: Some(phera_extension_api::DirectDimensions {
            read: |storage, offset| {
                let Value::Object(object) = offset else {
                    return None;
                };
                lookup(storage, &Key::Handle(object.id)).ok()
            },
            write: |state, storage, offset, info| {
                if CHECK && state.dimension_mutation_guarded() {
                    return false;
                }
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
        exists: |_, storage, value| {
            contains(storage, &Key::Handle(object(value, "offsetExists")?.id))
        },
        set: |context, storage, value, info| {
            require_mutation_allowed::<CHECK>(context)?;
            let object = object(value, "offsetSet")?;
            attach(storage, Key::Handle(object.id), object, info)
        },
        unset: |context, storage, value| {
            require_mutation_allowed::<CHECK>(context)?;
            detach(
                storage,
                &Key::Handle(object(value, "offsetUnset")?.id),
                false,
            )
        },
    }
}

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

fn invoke<'a, const CHECK: bool>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    operation: Operation,
    method: &'static str,
    replacement: Option<&'static str>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        if let Some(replacement) = replacement {
            context.diagnostics().deprecated(format!(
                "Method {CLASS}::{method}() is deprecated since 8.5, use method {CLASS}::{replacement}() instead"
            ));
        }
        let storage = receiver(this.as_ref())?;
        let mut arguments = arguments.into_iter();
        let object = object(arguments.next().unwrap_or(Value::Null), method)?;
        if matches!(operation, Operation::Set | Operation::Unset) {
            require_mutation_allowed::<CHECK>(context)?;
        }
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

pub(super) fn offset_set<const CHECK: bool>(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke::<CHECK>(context, this, args, Operation::Set, "offsetSet", None)
}

pub(super) fn offset_get(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke::<false>(context, this, args, Operation::Get, "offsetGet", None)
}

pub(super) fn offset_exists(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke::<false>(context, this, args, Operation::Exists, "offsetExists", None)
}

pub(super) fn offset_unset<const CHECK: bool>(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke::<CHECK>(context, this, args, Operation::Unset, "offsetUnset", None)
}

pub(super) fn attach_old<const CHECK: bool>(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke::<CHECK>(
        context,
        this,
        args,
        Operation::Set,
        "attach",
        Some("offsetSet"),
    )
}

pub(super) fn detach_old<const CHECK: bool>(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke::<CHECK>(
        context,
        this,
        args,
        Operation::Unset,
        "detach",
        Some("offsetUnset"),
    )
}

pub(super) fn contains_old(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    invoke::<false>(
        context,
        this,
        args,
        Operation::Exists,
        "contains",
        Some("offsetExists"),
    )
}
