//! `new CurlHandle()` is rejected. `curl_init()` creates the native handle.

use std::rc::Rc;

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, Context, MethodEntry, NativeClone, NativeError,
    NativeResult, Object, PhpArray, PropertyEntry, PropertyType, Registry, Value, Visibility,
};

const HANDLE_CTOR: &[MethodEntry] = &[MethodEntry::public("__construct", &[], construct_handle)];

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        methods: HANDLE_CTOR,
        hooks: Some(&ClassHooks {
            cast_object_id: true,
            clone: Some(NativeClone::Copy(clone_handle)),
            ..ClassHooks::EMPTY
        }),
        ..ClassEntry::class("CurlHandle")
    });
    const OPTIONS: &[PropertyEntry] =
        &[PropertyEntry::typed("options", PropertyType::Array, Visibility::Public).readonly()];
    const NON_CLONEABLE: ClassHooks = ClassHooks {
        clone: Some(NativeClone::Copy(reject_clone)),
        ..ClassHooks::EMPTY
    };
    const MULTI_HOOKS: ClassHooks = ClassHooks {
        cast_object_id: true,
        ..NON_CLONEABLE
    };
    for name in [
        "CurlMultiHandle",
        "CurlShareHandle",
        "CurlSharePersistentHandle",
    ] {
        registry.class(ClassEntry {
            kind: ClassKind::FinalClass,
            methods: HANDLE_CTOR,
            properties: if name == "CurlSharePersistentHandle" {
                OPTIONS
            } else {
                &[]
            },
            hooks: Some(if name == "CurlMultiHandle" {
                &MULTI_HOOKS
            } else {
                &NON_CLONEABLE
            }),
            ..ClassEntry::class(name)
        });
    }
    crate::upload::register(registry);
}

fn construct_handle(_: &mut dyn Context, object: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let name = object.map_or("CurlHandle", |object| object.class.name());
    let factory = match name {
        "CurlMultiHandle" => "curl_multi_init",
        "CurlShareHandle" => "curl_share_init",
        "CurlSharePersistentHandle" => "curl_share_init_persistent",
        _ => "curl_init",
    };
    Err(NativeError::error(format!(
        "Cannot directly construct {name}, use {factory}() instead"
    )))
}

pub(crate) fn create<T: 'static>(context: &mut dyn Context, class: &str, state: T) -> NativeResult {
    let value = context.create_object(class, PhpArray::new())?;
    let Value::Object(object) = &value else {
        return Err(NativeError::error("cURL factory did not return an object"));
    };
    install(object, state)?;
    Ok(value)
}

pub(crate) fn install<T: 'static>(object: &Object, state: T) -> Result<(), NativeError> {
    *object.native.try_borrow_mut().map_err(|_| unavailable())? = Some(Box::new(state));
    Ok(())
}

pub(crate) fn state<T: Clone + 'static>(object: &Object) -> Result<T, NativeError> {
    object
        .native
        .try_borrow()
        .map_err(|_| unavailable())?
        .as_ref()
        .and_then(|value| value.downcast_ref::<T>())
        .cloned()
        .ok_or_else(unavailable)
}

fn unavailable() -> NativeError {
    NativeError::error("cURL handle is unavailable")
}

fn clone_handle(original: &Rc<Object>, copy: &Object) -> Result<(), NativeError> {
    install(copy, crate::easy::copy(original)?)
}

fn reject_clone(original: &Rc<Object>, _: &Object) -> Result<(), NativeError> {
    Err(NativeError::error(format!(
        "Trying to clone an uncloneable object of class {}",
        original.class.name()
    )))
}
