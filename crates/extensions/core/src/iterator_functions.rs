//! Iterator helpers have the same method order as PHP's SPL.
//!
//! ```php
//! iterator_count($items);                  // rewind, valid, next, ...
//! iterator_to_array($items, false);         // also reads current()
//! iterator_apply($items, fn() => false);   // counts its final callback
//! ```

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
    traversal::{self, IteratorStep},
    value::to_bool,
};

const ITERABLE: Param = Param::new("iterator", ParamType::Iterable);

pub(crate) fn register(registry: &mut Registry) {
    const ARRAY: &[Param] = &[
        ITERABLE,
        Param::new("preserve_keys", ParamType::Bool).optional(),
    ];
    const COUNT: &[Param] = &[ITERABLE];
    const APPLY: &[Param] = &[
        Param::new("iterator", ParamType::Class("Traversable")),
        Param::new("callback", ParamType::Callable),
        Param::new("args", ParamType::Array).nullable().optional(),
    ];
    registry.function_with_callback_frame("iterator_to_array", ARRAY, collect);
    registry.function_with_callback_frame("iterator_count", COUNT, count);
    registry.function_with_callback_frame("iterator_apply", APPLY, apply);
}

fn collect(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let subject = args::value(&arguments, 0);
        let preserve = arguments.get(1).is_none_or(to_bool);
        let mut array = PhpArray::new();
        match subject {
            Value::Array(items) if preserve => return Ok(Value::Array(items.clone())),
            Value::Array(items) => {
                for (_, item) in items.iter() {
                    array.push(item.clone())?;
                }
            }
            Value::Object(object) => {
                let mut cursor = context.traverse(object.clone()).await?;
                traversal::step(cursor.as_mut(), context, IteratorStep::Rewind).await?;
                while to_bool(
                    &traversal::step(cursor.as_mut(), context, IteratorStep::Valid).await?,
                ) {
                    let (value, key) =
                        traversal::current(cursor.as_mut(), context, preserve).await?;
                    if let Some(key) = key {
                        array.insert(array_key(context, &key)?, value);
                    } else {
                        array.push(value)?;
                    }
                    traversal::step(cursor.as_mut(), context, IteratorStep::Advance).await?;
                }
            }
            _ => return Err(NativeError::error("Invalid iterable")),
        }
        Ok(Value::from(array))
    })
}

fn array_key(context: &mut dyn Context, key: &Value) -> Result<ArrayKey, NativeError> {
    Ok(ArrayKey::from_value_diag(key, context.diagnostics())?)
}

fn count(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let subject = args::value(&arguments, 0);
        match subject {
            Value::Array(array) => Ok(Value::Int(array.len() as i64)),
            Value::Object(object) => {
                let mut total = 0;
                let mut cursor = context.traverse(object.clone()).await?;
                traversal::step(cursor.as_mut(), context, IteratorStep::Rewind).await?;
                while to_bool(
                    &traversal::step(cursor.as_mut(), context, IteratorStep::Valid).await?,
                ) {
                    total += 1;
                    if total == i64::MAX {
                        break;
                    }
                    traversal::step(cursor.as_mut(), context, IteratorStep::Advance).await?;
                }
                Ok(Value::Int(total))
            }
            _ => Err(NativeError::error("Invalid iterable")),
        }
    })
}

fn apply(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let Value::Object(object) = args::value(&arguments, 0) else {
            return Err(NativeError::error("Invalid Traversable"));
        };
        let callback = args::value(&arguments, 1);
        context.validate_callback("iterator_apply", 2, "callback", callback)?;
        let empty = PhpArray::new();
        let parameters = match args::value(&arguments, 2) {
            Value::Array(parameters) => parameters.as_ref(),
            _ => &empty,
        };
        let mut cursor = context.traverse(object.clone()).await?;
        traversal::step(cursor.as_mut(), context, IteratorStep::Rewind).await?;
        let mut total = 0i64;
        while to_bool(&traversal::step(cursor.as_mut(), context, IteratorStep::Valid).await?) {
            total = total.saturating_add(1);
            if !to_bool(&context.call_array(callback, parameters).await?) {
                break;
            }
            traversal::step(cursor.as_mut(), context, IteratorStep::Advance).await?;
        }
        Ok(Value::Int(total))
    })
}
