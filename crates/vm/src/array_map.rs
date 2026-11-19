//! Mapping enters PHP through the shared resolver: `array_map(fn($x) => $x * 2, [1, 2])`.

use std::rc::Rc;

use phera_extension_api::{NativeError, NativeResult};
use phera_value::{PhpArray, Value};

use crate::Vm;

impl Vm {
    /// Resolve before iteration so an empty input still rejects an invalid callback.
    pub(crate) async fn map_arrays(&mut self, arguments: Vec<Value>) -> NativeResult {
        if arguments.len() < 2 {
            return Err(NativeError::argument_count_error(format!(
                "array_map() expects at least 2 arguments, {} given",
                arguments.len()
            )));
        }
        let callback = &arguments[0];
        let callee = if callback.is_null() {
            None
        } else {
            if self.has_autoloaders() {
                Box::pin(self.autoload_callable(callback)).await?;
            }
            Some(self.resolve_callable(callback).map_err(|error| {
                self.invalid_callback("array_map", 1, "callback", callback, error, true)
            })?)
        };
        let arrays = map_arguments(&arguments)?;
        if callee.is_some() {
            self.native_callback_sites
                .push(crate::error_traces::NativeCallbackSite {
                    boundary: self.call_stack.depth(),
                    native: crate::error_traces::NativeTrace::new("array_map", &arguments),
                });
        }
        let result = self.map_rows(callee.as_ref(), &arrays).await;
        if callee.is_some() {
            self.native_callback_sites.pop();
        }
        result
    }

    async fn map_rows(
        &mut self,
        callee: Option<&crate::calls::call_state::Callee>,
        arrays: &[Rc<PhpArray>],
    ) -> NativeResult {
        let length = arrays.iter().map(|array| array.len()).max().unwrap_or(0);
        let mut iterators: Vec<_> = arrays.iter().map(|array| array.iter()).collect();
        let mut result = PhpArray::new();
        for _ in 0..length {
            let mut key = None;
            let mut values = iterators.iter_mut().map(|iterator| {
                iterator
                    .next()
                    .map(|(next, value)| {
                        key = Some(next.clone());
                        value.deref().clone()
                    })
                    .unwrap_or_default()
            });
            let value = if let Some(callee) = callee {
                self.invoke_resolved_callable(callee.clone(), values)
                    .await?
            } else if arrays.len() == 1 {
                values.next().unwrap_or_default()
            } else {
                let mut tuple = PhpArray::new();
                for value in values {
                    let _ = tuple.push(value);
                }
                Value::Array(Rc::new(tuple))
            };
            if arrays.len() == 1 {
                if let Some(key) = key {
                    result.insert(key, value);
                }
            } else {
                let _ = result.push(value);
            }
        }
        Ok(Value::Array(Rc::new(result)))
    }
}

/// Array validation happens before iteration, including when every input is empty.
fn map_arguments(arguments: &[Value]) -> Result<Vec<Rc<PhpArray>>, NativeError> {
    let mut arrays = Vec::new();
    for (index, value) in arguments.iter().enumerate().skip(1) {
        let Value::Array(array) = &*value.deref() else {
            return Err(NativeError::type_error(format!(
                "array_map(): Argument #{} ($array) must be of type array, {} given",
                index + 1,
                value.type_name()
            )));
        };
        arrays.push(array.clone());
    }
    Ok(arrays)
}
