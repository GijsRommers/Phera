//! CachingIterator keeps a lookahead and can retain every visited row by key.
//!
//! ```php
//! $c = new CachingIterator($iterator, CachingIterator::FULL_CACHE);
//! foreach ($c as $value) {}
//! var_dump($c->getCache());
//! ```

mod dimensions;
mod registration;

pub(super) use registration::register;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args,
    traversal::IteratorStep,
    value::{to_bool, to_php_string},
};
use std::rc::Rc;

use super::{Wrapper, fetch, inner, new_wrapper, read, rewind, step, update};
use crate::{This, receiver};

const CLASS: &str = "CachingIterator";
const RECURSIVE: &str = "RecursiveCachingIterator";
const FULL_CACHE: i64 = 256;
const CATCH_CHILD: i64 = 16;

#[derive(Clone, Copy)]
enum StringMode {
    Off,
    CaptureValue,
    CaptureInner,
    UseKey,
    UseCurrent,
}

#[derive(Clone, Copy)]
struct Flags {
    raw: i64,
    strings: StringMode,
}

impl Flags {
    fn parse(raw: i64, constructor: bool, class: &str) -> Result<Self, NativeError> {
        let strings = match raw & 15 {
            0 => StringMode::Off,
            1 => StringMode::CaptureValue,
            2 => StringMode::UseKey,
            4 => StringMode::UseCurrent,
            8 => StringMode::CaptureInner,
            _ => {
                let (method, index) = if constructor {
                    ("__construct", 2)
                } else {
                    ("setFlags", 1)
                };
                return Err(NativeError::value_error(format!(
                    "{class}::{method}(): Argument #{index} ($flags) must contain only one of \
                     CachingIterator::CALL_TOSTRING, CachingIterator::TOSTRING_USE_KEY, \
                     CachingIterator::TOSTRING_USE_CURRENT, or CachingIterator::TOSTRING_USE_INNER"
                )));
            }
        };
        Ok(Self {
            raw: raw & 0xffff,
            strings,
        })
    }

    fn full_cache(self) -> bool {
        self.raw & FULL_CACHE != 0
    }
}

pub(super) struct Cache {
    flags: Flags,
    rows: PhpArray,
    child: Value,
    string: Value,
}

fn cache<T>(object: &Object, f: impl FnOnce(&Cache) -> T) -> Result<T, NativeError> {
    read(object, |wrapper| wrapper.cache.as_ref().map(f))?.ok_or_else(super::uninitialized)
}

fn change<T>(object: &Object, f: impl FnOnce(&mut Cache) -> T) -> Result<T, NativeError> {
    update(object, |wrapper| wrapper.cache.as_mut().map(f))?.ok_or_else(super::uninitialized)
}

fn construct(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let class = if object.class.is_instance_of(RECURSIVE) {
            RECURSIVE
        } else {
            CLASS
        };
        let flags = Flags::parse(args::int(&arguments, 1).unwrap_or(1), true, class)?;
        let source = args::object(&arguments, 0).ok_or_else(super::uninitialized)?;
        new_wrapper(
            context,
            object,
            source.clone(),
            Wrapper {
                cache: Some(Cache {
                    flags,
                    rows: PhpArray::new(),
                    child: Value::Null,
                    string: Value::from(""),
                }),
                ..Wrapper::default()
            },
        )
        .await
    })
}

async fn capture_child(context: &mut dyn Context, object: &Object, flags: Flags) -> NativeResult {
    let inner = inner(object)?;
    let has_child = context
        .call_method(inner.clone(), "hasChildren", Vec::new())
        .await?;
    if !to_bool(&has_child) {
        return Ok(Value::Null);
    }
    let child = context
        .call_method(inner, "getChildren", Vec::new())
        .await?;
    let iterator = context.create_object(RECURSIVE, PhpArray::new())?;
    let Value::Object(object) = &iterator else {
        return Err(NativeError::error(
            "Cannot create recursive caching iterator",
        ));
    };
    context
        .call_method(
            object.clone(),
            "__construct",
            vec![child, Value::Int(flags.raw)],
        )
        .await?;
    Ok(iterator)
}

async fn capture(context: &mut dyn Context, object: &Object) -> NativeResult {
    change(object, |cache| {
        cache.child = Value::Null;
        cache.string = Value::from("");
    })?;
    if !fetch(context, object).await? {
        return Ok(Value::Null);
    }
    let flags = cache(object, |cache| cache.flags)?;
    let (value, key) = read(object, |state| {
        (state.value.clone().unwrap_or_default(), state.key.clone())
    })?;
    if flags.full_cache() {
        let key = ArrayKey::from_value_diag(&key, context.diagnostics())?;
        change(object, |cache| {
            cache.rows.insert(key, value.deref().clone());
        })?;
    }
    if object.class.is_instance_of(RECURSIVE) {
        match capture_child(context, object, flags).await {
            Ok(child) => change(object, |cache| cache.child = child)?,
            Err(error) if flags.raw & CATCH_CHILD == 0 => return Err(error),
            Err(_) => {}
        }
    }
    let string = match flags.strings {
        StringMode::CaptureValue => Some(value),
        StringMode::CaptureInner => Some(Value::Object(inner(object)?)),
        _ => None,
    };
    if let Some(string) = string {
        let result = stringify(context, &string).await?;
        change(object, |cache| cache.string = result)?;
    }
    step(context, object, IteratorStep::Advance).await?;
    Ok(Value::Null)
}

pub(crate) async fn stringify(context: &mut dyn Context, value: &Value) -> NativeResult {
    if let Value::Object(object) = &*value.deref()
        && object.class.is_instance_of("Stringable")
    {
        return context
            .call_method(object.clone(), "__toString", Vec::new())
            .await;
    }
    Ok(Value::String(to_php_string(value, context.diagnostics())?))
}

fn on_rewind(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        rewind(context, object).await?;
        change(object, |cache| cache.rows = PhpArray::new())?;
        capture(context, object).await
    })
}

fn on_next(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { capture(context, receiver(this.as_ref())?).await })
}

fn has_next(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let result = step(context, receiver(this.as_ref())?, IteratorStep::Valid).await?;
        Ok(Value::Bool(to_bool(&result)))
    })
}

fn as_string(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        match cache(object, |cache| cache.flags.strings)? {
            StringMode::Off => Err(NativeError::throw(
                "BadMethodCallException",
                format!(
                    "{} does not fetch string value (see CachingIterator::__construct)",
                    object.class.name()
                ),
            )),
            StringMode::CaptureValue | StringMode::CaptureInner => {
                cache(object, |cache| cache.string.clone())
            }
            StringMode::UseKey => {
                let value = read(object, |state| state.key.clone())?;
                stringify(context, &value).await
            }
            StringMode::UseCurrent => {
                let value = read(object, |state| state.value.clone().unwrap_or_default())?;
                stringify(context, &value).await
            }
        }
    })
}

fn get_flags(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let valid = read(object, |state| state.value.is_some())?;
    cache(object, |cache| {
        Value::Int(cache.flags.raw | if valid { 65536 } else { 0 })
    })
}

fn set_flags(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let flags = Flags::parse(args::int(args, 0).unwrap_or_default(), false, CLASS)?;
    change(object, |cache| {
        match (cache.flags.strings, flags.strings) {
            (StringMode::CaptureValue, other) if !matches!(other, StringMode::CaptureValue) => {
                return Err(NativeError::throw(
                    "InvalidArgumentException",
                    "Unsetting flag CALL_TO_STRING is not possible",
                ));
            }
            (StringMode::CaptureInner, other) if !matches!(other, StringMode::CaptureInner) => {
                return Err(NativeError::throw(
                    "InvalidArgumentException",
                    "Unsetting flag TOSTRING_USE_INNER is not possible",
                ));
            }
            _ => {}
        }
        if flags.full_cache() && !cache.flags.full_cache() {
            cache.rows = PhpArray::new();
        }
        cache.flags = flags;
        Ok(Value::Null)
    })?
}

fn get_children(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    cache(receiver(this)?, |cache| cache.child.clone())
}

fn has_children(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    cache(receiver(this)?, |cache| Value::Bool(!cache.child.is_null()))
}
