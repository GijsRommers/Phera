//! WeakReference and WeakMap keep object keys without extending their lifetime.
//!
//! ```php
//! $reference = WeakReference::create($object);
//! unset($object);
//! var_dump($reference->get()); // null
//! ```

mod cursor;
mod references;

use std::{
    collections::HashMap,
    rc::{Rc, Weak},
};

use indexmap::IndexMap;
use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, ClassKind, Context, MethodEntry as Method, NativeClone,
    NativeDimensions, NativeError, NativePropertyAccess, NativePropertyMutation, NativeResult,
    Object, Param, ParamType, PhpArray, PropertyPurpose, Registry, Value, args,
};

use crate::{This, make, read_state, receiver, store, update_state};

#[derive(Default)]
struct References(HashMap<*const Object, Weak<Object>>);

fn construct(_: &mut dyn Context, _: This<'_>, _: &[Value]) -> NativeResult {
    Err(NativeError::error(
        "Direct instantiation of WeakReference is not allowed, use WeakReference::create instead",
    ))
}

fn create(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    let target = args::object(arguments, 0)
        .ok_or_else(|| NativeError::error("Missing WeakReference target"))?;
    let key = Rc::as_ptr(target);
    if let Some(existing) = context
        .state()
        .get_or_default::<References>()
        .0
        .get(&key)
        .and_then(Weak::upgrade)
    {
        return Ok(Value::Object(existing));
    }
    let reference = make(context, "WeakReference", Rc::downgrade(target))?;
    context
        .state()
        .get_or_default::<References>()
        .0
        .insert(key, Rc::downgrade(&reference));
    Ok(Value::Object(reference))
}

fn get_reference(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    target_value(receiver(this)?)
}

fn target_value(reference: &Object) -> NativeResult {
    read_state(reference, |target: &Weak<Object>| {
        target.upgrade().map_or(Value::Null, Value::Object)
    })
}

fn reference_view(
    _: &mut dyn Context,
    reference: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    if purpose == PropertyPurpose::Debug {
        Ok([(ArrayKey::str("object"), target_value(reference)?)]
            .into_iter()
            .collect())
    } else {
        Ok(reference.to_array())
    }
}

#[derive(Clone, Default)]
struct Map(IndexMap<u32, (Weak<Object>, Value)>);

fn key(offset: Value, append: bool) -> Result<Rc<Object>, NativeError> {
    match &*offset.deref() {
        Value::Object(object) => Ok(object.clone()),
        Value::Null if append => Err(NativeError::error("Cannot append to WeakMap")),
        _ => Err(NativeError::type_error("WeakMap key must be an object")),
    }
}

fn get(_: &mut dyn Context, map: &Object, offset: Value) -> NativeResult {
    let key = key(offset, false)?;
    let result = read_state(map, |state: &Map| {
        state
            .0
            .get(&key.id)
            .filter(|(weak, _)| weak.as_ptr() == Rc::as_ptr(&key))
            .map(|(_, value)| value.deref().clone())
    })?;
    result.ok_or_else(|| missing_key(&key))
}

fn missing_key(key: &Object) -> NativeError {
    NativeError::error(format!(
        "Object {}#{} not contained in WeakMap",
        key.class.name(),
        key.id,
    ))
}

fn exists(_: &mut dyn Context, map: &Object, offset: Value) -> Result<bool, NativeError> {
    let key = key(offset, false)?;
    read_state(map, |state: &Map| {
        state
            .0
            .get(&key.id)
            .is_some_and(|(weak, value)| weak.as_ptr() == Rc::as_ptr(&key) && !value.is_null())
    })
}

fn set(_: &mut dyn Context, map: &Object, offset: Value, value: Value) -> Result<(), NativeError> {
    let key = key(offset, true)?;
    update_state(map, |state: &mut Map| {
        if state
            .0
            .get(&key.id)
            .is_some_and(|(weak, _)| weak.strong_count() == 0)
        {
            state.0.shift_remove(&key.id);
        }
        state
            .0
            .insert(key.id, (Rc::downgrade(&key), value.deref().clone()));
    })
}

fn unset(_: &mut dyn Context, map: &Object, offset: Value) -> Result<(), NativeError> {
    let key = key(offset, false)?;
    update_state(map, |state: &mut Map| {
        if state
            .0
            .get(&key.id)
            .is_some_and(|(weak, _)| weak.as_ptr() == Rc::as_ptr(&key))
        {
            state.0.shift_remove(&key.id);
        }
    })
}

fn offset_get(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    get(context, receiver(this)?, args::value(args, 0).clone())
}

fn offset_exists(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    exists(context, receiver(this)?, args::value(args, 0).clone()).map(Value::Bool)
}

fn offset_set(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    if args::value(args, 0).is_null() {
        return Err(NativeError::type_error("WeakMap key must be an object"));
    }
    set(
        context,
        receiver(this)?,
        args::value(args, 0).clone(),
        args::value(args, 1).clone(),
    )?;
    Ok(Value::Null)
}

fn offset_unset(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    unset(context, receiver(this)?, args::value(args, 0).clone())?;
    Ok(Value::Null)
}

fn native_count(_: &mut dyn Context, map: &Object) -> Result<i64, NativeError> {
    update_state(map, |state: &mut Map| {
        state.0.retain(|_, (target, _)| target.strong_count() != 0);
        state.0.len() as i64
    })
}

fn count(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    native_count(context, receiver(this)?).map(Value::Int)
}

fn clone_map(original: &Rc<Object>, copy: &Object) -> Result<(), NativeError> {
    store(copy, read_state(original, Map::clone)?)
}

fn view(
    _: &mut dyn Context,
    map: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    match purpose {
        PropertyPurpose::Debug => read_state(map, |state: &Map| {
            state
                .0
                .values()
                .filter_map(|(key, value)| Some((key.upgrade()?, value)))
                .enumerate()
                .map(|(index, (key, value))| {
                    let pair = [
                        (ArrayKey::str("key"), Value::Object(key)),
                        (ArrayKey::str("value"), value.clone()),
                    ]
                    .into_iter()
                    .collect::<PhpArray>();
                    (ArrayKey::Int(index as i64), Value::from(pair))
                })
                .collect()
        }),
        PropertyPurpose::Comparison => read_state(map, |state: &Map| {
            state
                .0
                .iter()
                .filter(|(_, (key, _))| key.strong_count() != 0)
                .map(|(id, (_, value))| (ArrayKey::Int(i64::from(*id)), value.clone()))
                .collect()
        }),
        _ => Ok(map.to_array()),
    }
}

const KEY: Param = Param::new("object", ParamType::Mixed);
const VALUE: Param = Param::new("value", ParamType::Mixed);
const TARGET: Param = Param::new("object", ParamType::Object);

const NO_DYNAMIC_PROPERTIES: NativePropertyAccess = NativePropertyAccess {
    read: |_, _, _, _| Ok(None),
    mutate: |_, object, name, mutation| match mutation {
        NativePropertyMutation::Unset => Ok(false),
        _ => Err(NativeError::error(format!(
            "Cannot create dynamic property {}::${name}",
            object.class.name(),
        ))),
    },
};

const MAP_HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Copy(clone_map)),
    properties: Some(view),
    property_access: Some(&NO_DYNAMIC_PROPERTIES),
    iterator: Some(cursor::by_value),
    iterator_by_reference: Some(cursor::by_reference),
    dimensions: Some(NativeDimensions {
        direct: None,
        reference: Some(references::reference),
        bind: Some(references::bind),
        fallback_if_overridden: &[],
        get,
        exists,
        set,
        unset,
    }),
    count: Some(native_count),
    ..ClassHooks::EMPTY
};

const REFERENCE_HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    properties: Some(reference_view),
    property_access: Some(&NO_DYNAMIC_PROPERTIES),
    ..ClassHooks::EMPTY
};

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        hooks: Some(&REFERENCE_HOOKS),
        methods: const {
            &[
                Method::public("__construct", &[], construct),
                Method::public("create", &[TARGET], create)
                    .static_()
                    .returning("WeakReference"),
                Method::public("get", &[], get_reference).returning("?object"),
            ]
        },
        ..ClassEntry::class("WeakReference")
    });
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        interfaces: &["ArrayAccess", "Countable", "IteratorAggregate"],
        hooks: Some(&MAP_HOOKS),
        methods: const {
            &[
                Method::public("offsetGet", &[KEY], offset_get).returning("mixed"),
                Method::public("offsetExists", &[KEY], offset_exists).returning("bool"),
                Method::public("offsetSet", &[KEY, VALUE], offset_set).returning("void"),
                Method::public("offsetUnset", &[KEY], offset_unset).returning("void"),
                Method::public("count", &[], count).returning("int"),
                Method::public("getIterator", &[], cursor::get_iterator).returning("Iterator"),
            ]
        },
        ..ClassEntry::class("WeakMap")
    });
}
