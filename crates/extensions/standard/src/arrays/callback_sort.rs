//! User sorting: `usort($a, 'strcmp')` renumbers keys; `uasort` retains them.

use std::cmp::Ordering;

use phera_extension_api::{
    AsyncFn, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    PhpArray, Registry, Value, args,
};

pub(crate) fn register(registry: &mut Registry) {
    for (name, handler) in [
        ("usort", usort as AsyncFn),
        ("uasort", uasort),
        ("uksort", uksort),
    ] {
        registry.function(FunctionEntry::asynchronous(
            name,
            const {
                &[
                    Param::new("array", ParamType::Array).by_ref(),
                    Param::new("callback", ParamType::Callable),
                ]
            },
            handler,
        ));
    }
}

fn usort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, arguments, "usort"))
}
fn uasort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, arguments, "uasort"))
}
fn uksort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, arguments, "uksort"))
}

async fn run(context: &mut dyn Context, arguments: Vec<Value>, name: &str) -> NativeResult {
    let callback = args::value(&arguments, 1);
    super::callback::validate(name, 2, true, callback, false)?;
    let target = args::reference(&arguments, 0)
        .ok_or_else(|| NativeError::error("Expected array reference"))?;
    let entries: Vec<_> = {
        let value = target
            .try_borrow()
            .map_err(|_| NativeError::error("Array is already being modified"))?;
        let Value::Array(array) = &*value else {
            return Err(NativeError::error("Expected array"));
        };
        array
            .iter()
            .map(|(key, value)| (key, value.clone()))
            .collect()
    };
    if entries.is_empty() {
        return Ok(Value::Bool(true));
    }
    let mut comparison = super::callback::Comparison {
        name,
        deprecated: false,
    };
    let mut failure = None;
    // Zend finishes its traversal after an exception. Further callback attempts
    // compare equal, with original ordinals breaking ties, then commits the copy.
    let indices = super::sort_order_async::order(entries.len(), async |left, right| {
        let values = if name == "uksort" {
            (
                super::key_value(&entries[left].0),
                super::key_value(&entries[right].0),
            )
        } else {
            (entries[left].1.clone(), entries[right].1.clone())
        };
        let order = if failure.is_some() {
            Ordering::Equal
        } else {
            match comparison
                .compare(context, callback, values.0, values.1, true)
                .await
            {
                Ok(order) => order,
                Err(error) => {
                    failure = Some(error);
                    Ordering::Equal
                }
            }
        };
        Ok(order.then_with(|| left.cmp(&right)))
    })
    .await?;
    let mut sorted = PhpArray::with_capacity(entries.len());
    for index in indices {
        let (key, value) = &entries[index];
        if name == "usort" {
            sorted.push(value.clone())?;
        } else {
            sorted.insert(key.clone(), value.clone());
        }
    }
    *target
        .try_borrow_mut()
        .map_err(|_| NativeError::error("Array is already being modified"))? = Value::from(sorted);
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(Value::Bool(true))
}
