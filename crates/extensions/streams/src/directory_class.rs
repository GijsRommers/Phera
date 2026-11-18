//! `dir('src')->read()` reads an entry through PHP's Directory object.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassKind, ConstantValue, Context, LocalFuture, MethodEntry, NativeError,
    NativeResult, Object, PropertyEntry, Registry, Value, Visibility, value::Resource,
};

use crate::directory::{is_open, read, rewind};

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        properties: const {
            &[
                PropertyEntry::new("path", ConstantValue::Null, Visibility::Public).readonly(),
                PropertyEntry::new("handle", ConstantValue::Null, Visibility::Public).readonly(),
            ]
        },
        methods: const {
            &[
                MethodEntry::public("__construct", &[], construct),
                MethodEntry::public_async("read", &[], object_read),
                MethodEntry::public_async("rewind", &[], object_rewind),
                MethodEntry::public("close", &[], object_close),
            ]
        },
        ..ClassEntry::class("Directory")
    });
}

fn construct(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Err(NativeError::error(
        "Cannot directly construct Directory, use dir() instead",
    ))
}

fn object_handle(object: Option<&Rc<Object>>, action: &str) -> Result<Rc<Resource>, NativeError> {
    if let Some(Value::Resource(handle)) = object.and_then(|o| o.get(&ArrayKey::str("handle")))
        && is_open(&handle)
    {
        return Ok(handle);
    }
    Err(NativeError::type_error(format!(
        "Directory::{action}(): cannot use Directory resource after it has been closed"
    )))
}

fn object_read(
    _: &mut dyn Context,
    object: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { read(object_handle(object.as_ref(), "read")?.as_ref()).await })
}

fn object_rewind(
    _: &mut dyn Context,
    object: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { rewind(object_handle(object.as_ref(), "rewind")?.as_ref()).await })
}

fn object_close(_: &mut dyn Context, object: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    object_handle(object, "close")?.data.borrow_mut().take();
    Ok(Value::Null)
}
