//! Filters cache a row before calling the PHP predicate.
//!
//! ```php
//! $even = new CallbackFilterIterator($numbers, fn($n) => $n % 2 === 0);
//! ```

use std::rc::Rc;

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, Value, args, value::to_bool,
};

use super::{fetch, inner, next, read, rewind, source, step};
use crate::receiver;

pub(super) fn construct_callback(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let callback = args::value(&arguments, 1).deref().clone();
        let name = if object
            .class
            .is_instance_of("RecursiveCallbackFilterIterator")
        {
            "RecursiveCallbackFilterIterator::__construct"
        } else {
            "CallbackFilterIterator::__construct"
        };
        context.validate_callback(name, 2, "callback", &callback)?;
        source(context, object, &arguments, Some(callback)).await
    })
}

pub(super) fn accept(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let (value, key, callback) = read(object, |state| {
            (
                state.value.clone(),
                state.key.clone(),
                state.callback.clone(),
            )
        })?;
        let Some(value) = value else {
            return Ok(Value::Bool(false));
        };
        let callback = callback.ok_or_else(super::uninitialized)?;
        let args = vec![value, key, Value::Object(inner(object)?)];
        context.push_callback_frame("CallbackFilterIterator->accept", this, Vec::new());
        let result = context.call(&callback, args).await;
        context.pop_callback_frame();
        Ok(Value::Bool(to_bool(&result?)))
    })
}

pub(super) async fn advance_to_match(
    context: &mut dyn Context,
    object: &Rc<Object>,
) -> NativeResult {
    while fetch(context, object).await? {
        let accepted = context
            .call_method(object.clone(), "accept", Vec::new())
            .await?;
        if to_bool(&accepted) {
            break;
        }
        step(
            context,
            object,
            phera_extension_api::traversal::IteratorStep::Advance,
        )
        .await?;
    }
    Ok(Value::Null)
}

pub(super) fn on_rewind(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        context.push_callback_frame("FilterIterator->rewind", this.clone(), Vec::new());
        let result = async {
            rewind(context, object).await?;
            advance_to_match(context, object).await
        }
        .await;
        context.pop_callback_frame();
        result
    })
}

pub(super) fn on_next(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        context.push_callback_frame("FilterIterator->next", this.clone(), Vec::new());
        let result = async {
            next(context, object).await?;
            advance_to_match(context, object).await
        }
        .await;
        context.pop_callback_frame();
        result
    })
}

pub(super) fn has_children(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let result = context
            .call_method(inner(receiver(this.as_ref())?)?, "hasChildren", Vec::new())
            .await?;
        Ok(Value::Bool(to_bool(&result)))
    })
}

pub(super) fn children(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let inner_child = context
            .call_method(inner(object)?, "getChildren", Vec::new())
            .await?;
        if inner_child.is_null() {
            return Ok(Value::Null);
        }
        let result = context.create_object(object.class.name(), PhpArray::new())?;
        let Value::Object(child) = &result else {
            return Err(NativeError::error("Cannot create recursive filter"));
        };
        let mut args = vec![inner_child];
        if let Some(callback) = read(object, |state| state.callback.clone())? {
            args.push(callback);
        }
        context
            .call_method(child.clone(), "__construct", args)
            .await?;
        Ok(result)
    })
}
