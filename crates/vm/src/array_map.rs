//! Mapping enters PHP through the shared resolver: `array_map(fn($x) => $x * 2, [1, 2])`.

use std::rc::Rc;

use phera_extension_api::{NativeError, NativeResult};
use phera_value::{PhpArray, Value};

use crate::Vm;

impl Vm {
    /// Resolve before iteration so an empty input still rejects an invalid callback.
    pub(crate) async fn map_arrays(&mut self, arguments: Vec<Value>) -> NativeResult {
        let prepared = self.prepare_array_map(&arguments).await;
        let (callee, arrays) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => return self.finish_array_map(Err(error), arguments),
        };
        let Some(callee) = callee.as_ref() else {
            let result = self.map_rows(None, &arrays).await;
            return self.finish_array_map(result, arguments);
        };
        self.push_native_callback(crate::error_traces::NativeCaller::ArrayMap(arguments));
        let result = self.map_rows(Some(callee), &arrays).await;
        if let Some(crate::error_traces::NativeCallbackSite {
            native: crate::error_traces::NativeCaller::ArrayMap(arguments),
            ..
        }) = self.native_callback_sites.pop()
        {
            return self.finish_array_map(result, arguments);
        }
        result
    }

    async fn prepare_array_map(
        &mut self,
        arguments: &[Value],
    ) -> Result<(Option<crate::calls::call_state::Callee>, Vec<Rc<PhpArray>>), NativeError> {
        let [callback, _, ..] = arguments else {
            return Err(NativeError::argument_count_error(format!(
                "array_map() expects at least 2 arguments, {} given",
                arguments.len()
            )));
        };
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
        Ok((callee, map_arguments(arguments)?))
    }

    /// Keep the array_map frame on a fresh exception, reusing the arguments the
    /// intrinsic already owns while it invokes its callbacks.
    fn finish_array_map(&mut self, result: NativeResult, arguments: Vec<Value>) -> NativeResult {
        if let Err(error) = &result {
            self.record_native_failure(
                error,
                crate::calls::native_frames::NativeFrame::Function("array_map"),
                &arguments,
            );
        }
        self.call_stack.recycle_native_arguments(arguments);
        result
    }

    async fn map_rows(
        &mut self,
        callee: Option<&crate::calls::call_state::Callee>,
        arrays: &[Rc<PhpArray>],
    ) -> NativeResult {
        let length = arrays.iter().map(|array| array.len()).max().unwrap_or(0);
        let mut iterators: Vec<_> = arrays.iter().map(|array| array.iter()).collect();
        let mut result = PhpArray::with_capacity(length);
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
                Value::Array(Rc::new(PhpArray::from_values(values, arrays.len())))
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
