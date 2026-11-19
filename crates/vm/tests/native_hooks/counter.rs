//! `clone $counter` copies the number stored outside the PHP property table.

use std::rc::Rc;

use phera_extension_api::value::{Diagnostics, OpError};
use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, Context, MethodEntry, NativeClone, NativeComparison,
    NativeError, NativeResult, Object, Param, ParamType, PhpArray, PropertyEntry, PropertyPurpose,
    PropertyType, Registry, Value, Visibility, args,
};

const CLASS: &str = "EngineCounter";
const ARGUMENTS: &[Param] = &[Param::new("value", ParamType::Int)];

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        properties: const {
            &[PropertyEntry::typed("stamp", PropertyType::Int, Visibility::Public).readonly()]
        },
        methods: const {
            &[
                MethodEntry::public("__construct", ARGUMENTS, construct),
                MethodEntry::public("read", &[], read),
                MethodEntry::public("stamp", &[], stamp),
                MethodEntry::public("make", ARGUMENTS, make).static_(),
                MethodEntry::public(
                    "accept",
                    const { &[Param::new("counter", ParamType::Class(CLASS))] },
                    accept,
                )
                .static_(),
            ]
        },
        hooks: const {
            Some(&ClassHooks {
                clone: Some(NativeClone::Copy(copy)),
                compare: Some(compare),
                properties: Some(project),
                ..ClassHooks::EMPTY
            })
        },
        ..ClassEntry::class(CLASS)
    });
}

fn counter(object: &Object) -> Option<i64> {
    object
        .native
        .try_borrow()
        .ok()?
        .as_ref()?
        .downcast_ref::<i64>()
        .copied()
}

fn missing() -> NativeError {
    NativeError::error(
        "Typed property EngineCounter::$value must not be accessed before initialization",
    )
}

fn store(object: &Object, value: i64) -> Result<(), NativeError> {
    let mut state = object
        .native
        .try_borrow_mut()
        .map_err(|_| NativeError::error("Counter is borrowed"))?;
    *state = Some(Box::new(value));
    Ok(())
}

fn construct(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    store(
        this.ok_or_else(missing)?,
        args::int(arguments, 0).ok_or_else(missing)?,
    )?;
    Ok(Value::Null)
}

fn read(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    counter(this.ok_or_else(missing)?)
        .map(Value::Int)
        .ok_or_else(missing)
}

fn stamp(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    this.ok_or_else(missing)?
        .set(ArrayKey::str("stamp"), Value::Int(7));
    Ok(Value::Null)
}

fn make(context: &mut dyn Context, _: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let class = context.called_class().unwrap_or(CLASS).to_owned();
    let value = context.create_object(&class, PhpArray::new())?;
    let Value::Object(object) = &value else {
        return Err(missing());
    };
    store(object, args::int(arguments, 0).ok_or_else(missing)?)?;
    Ok(value)
}

fn accept(_: &mut dyn Context, _: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    counter(args::object(arguments, 0).ok_or_else(missing)?)
        .map(Value::Int)
        .ok_or_else(missing)
}

fn copy(object: &std::rc::Rc<Object>, copy: &Object) -> Result<(), NativeError> {
    if let Some(value) = counter(object) {
        store(copy, value)?;
    }
    Ok(())
}

fn compare(
    left: &Object,
    right: &Object,
    _: &mut dyn Diagnostics,
) -> Result<NativeComparison, OpError> {
    if left.class.name() != CLASS || right.class.name() != CLASS {
        return Ok(NativeComparison::Default);
    }
    Ok(NativeComparison::Ordered(
        counter(left).cmp(&counter(right)),
    ))
}

fn project(
    _: &mut dyn Context,
    object: &Object,
    _: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let mut properties = PhpArray::new();
    if let Some(value) = counter(object) {
        properties.insert(ArrayKey::str("value"), Value::Int(value));
    }
    for (key, value) in object.to_array().iter() {
        properties.insert(key, value.clone());
    }
    Ok(properties)
}
