//! AppendIterator chains iterators and returns its live list of sources.
//!
//! ```php
//! $chain = new AppendIterator;
//! $chain->append(new ArrayIterator([1]));
//! $chain->append(new ArrayIterator([2]));
//! ```

use phera_extension_api::{
    ClassEntry, Context, LocalFuture, MethodEntry as Method, NativeError, NativeResult, Object,
    Param, ParamType, Registry,
};
use std::rc::Rc;

use super::{Wrapper, fetch, methods, next, read, rewind, update};
use crate::{This, arrays, receiver, store};
use phera_extension_api::Value;

pub(super) struct Append {
    sources: Rc<Object>,
    slot: Option<usize>,
}

fn construct(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    if read(object, |_| ()).is_ok() {
        return Err(NativeError::throw(
            "BadMethodCallException",
            "Cannot call constructor twice",
        ));
    }
    let sources = arrays::empty_iterator(context)?;
    store(
        object,
        Wrapper {
            append: Some(Append {
                sources,
                slot: None,
            }),
            ..Wrapper::default()
        },
    )?;
    Ok(Value::Null)
}

fn sources(object: &Object) -> Result<Rc<Object>, NativeError> {
    read(object, |state| {
        state.append.as_ref().map(|state| state.sources.clone())
    })?
    .ok_or_else(super::uninitialized)
}

fn select_slot(object: &Object, slot: Option<usize>) -> Result<(), NativeError> {
    update(object, |state| {
        if let Some(append) = &mut state.append {
            append.slot = slot;
        }
    })
}

async fn activate(
    context: &mut dyn Context,
    object: &Rc<Object>,
    after: Option<usize>,
) -> NativeResult {
    let sources = arrays::backing(sources(object)?.as_ref())?;
    let mut position = match after {
        Some(slot) => sources.next_position(slot),
        None => sources.first_position(),
    };
    while let Some(slot) = position {
        let Some((_, value)) = sources.entry_at(slot) else {
            break;
        };
        let Value::Object(inner) = &*value.deref() else {
            return Err(NativeError::throw(
                "LogicException",
                "Iterator is not an Iterator",
            ));
        };
        if Rc::ptr_eq(inner, object) || !inner.class.is_instance_of("Iterator") {
            return Err(NativeError::throw(
                "LogicException",
                "Iterator is not an Iterator",
            ));
        }
        let cursor = context.traverse(inner.clone()).await?;
        update(object, |state| {
            state.inner = Some(inner.clone());
            state.cursor = Some(cursor);
        })?;
        select_slot(object, Some(slot))?;
        rewind(context, object).await?;
        if fetch(context, object).await? {
            return Ok(Value::Null);
        }
        position = sources.next_position(slot);
    }
    select_slot(object, None)?;
    Ok(Value::Null)
}

fn append(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let inner = arguments
            .first()
            .ok_or_else(super::uninitialized)?
            .deref()
            .clone();
        let sources = sources(object)?;
        let previous = arrays::backing(&sources)?;
        let last = previous.end_position().checked_sub(1);
        arrays::push(context, &sources, inner)?;
        if read(object, |state| state.value.is_none())? {
            activate(context, object, last).await?;
        }
        Ok(Value::Null)
    })
}

fn on_rewind(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { activate(context, receiver(this.as_ref())?, None).await })
}

fn on_next(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let slot = read(object, |state| {
            state.append.as_ref().and_then(|state| state.slot)
        })?;
        if let Some(slot) = slot {
            next(context, object).await?;
            if !fetch(context, object).await? {
                activate(context, object, Some(slot)).await?;
            }
        }
        Ok(Value::Null)
    })
}

fn index(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let slot = read(object, |state| {
        state.append.as_ref().and_then(|state| state.slot)
    })?;
    let sources = arrays::backing(sources(object)?.as_ref())?;
    Ok(slot
        .and_then(|slot| sources.entry_at(slot))
        .map_or(Value::Null, |(key, _)| key.into()))
}

fn get_array(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    sources(receiver(this)?).map(Value::Object)
}

const ITERATOR: Param = Param::new("iterator", ParamType::Class("Iterator"));

pub(super) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        parent: Some("IteratorIterator"),
        methods: const {
            &[
                Method::public("__construct", &[], construct),
                Method::public_async("append", &[ITERATOR], append).tentative_return("void"),
                Method::public_async("rewind", &[], on_rewind).tentative_return("void"),
                Method::public_async("next", &[], on_next).tentative_return("void"),
                Method::public("valid", &[], methods::valid).tentative_return("bool"),
                Method::public("current", &[], methods::current).tentative_return("mixed"),
                Method::public("getIteratorIndex", &[], index).tentative_return("?int"),
                Method::public("getArrayIterator", &[], get_array)
                    .tentative_return("ArrayIterator"),
            ]
        },
        ..ClassEntry::class("AppendIterator")
    });
}
