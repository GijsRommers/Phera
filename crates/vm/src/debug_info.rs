//! `var_dump($object)` and `print_r($object)` show the array returned by
//! __debugInfo. `var_export($object)` exports actual properties as __set_state input.

use std::rc::Rc;

use phera_extension_api::{LocalFuture, NativeError, PropertyView};
use phera_value::{Object, Value};

use crate::Vm;

const INVALID_RESULT: &str = "__debuginfo() must return an array";

impl Vm {
    pub(crate) fn has_debug_info(&self, object: &Object) -> bool {
        object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
            .and_then(|class| class.magic.as_ref())
            .is_some_and(|magic| magic.debug_info.is_some())
    }

    pub(crate) fn debug_properties(
        &mut self,
        object: Rc<Object>,
        function: &'static str,
        arguments: &[Value],
    ) -> LocalFuture<'_, Result<PropertyView, NativeError>> {
        let native = crate::error_traces::NativeTrace::new(function, arguments);
        Box::pin(self.inspect_properties(object, native))
    }

    async fn inspect_properties(
        &mut self,
        object: Rc<Object>,
        native: crate::error_traces::NativeTrace,
    ) -> Result<PropertyView, NativeError> {
        if self.debug_objects.contains(&object.id) {
            return Ok(PropertyView::Recursive);
        }
        let Some(class) = self.classes.for_object(&object) else {
            return Ok(PropertyView::Snapshot(Rc::new(object.to_array())));
        };
        let Some(method) = class
            .magic
            .as_ref()
            .and_then(|magic| magic.debug_info.clone())
        else {
            return Ok(PropertyView::Snapshot(Rc::new(object.to_array())));
        };
        let qualified = format!("{}::{}()", method.declaring, method.name);
        self.debug_objects.push(object.id);
        self.native_callback_sites
            .push(crate::error_traces::NativeCallbackSite {
                boundary: self.call_stack.depth(),
                native: crate::error_traces::NativeCaller::Saved(native),
            });
        let result = self
            .invoke_magic_method(method, class, object, [])
            .await
            .map(|value| value.deref().clone());
        let result = match result {
            Ok(Value::Array(_) | Value::Null) | Err(_) => result,
            Ok(_) => Err(self.fatal_error(1, INVALID_RESULT, self.location())),
        };
        self.native_callback_sites.pop();
        self.debug_objects.pop();
        match result {
            Ok(Value::Array(properties)) => Ok(PropertyView::DebugInfo(properties)),
            Ok(Value::Null) => Err(NativeError::error(format!(
                "Returning null from {qualified} is not supported by Phera: deprecated since PHP 8.5, return an empty array instead"
            ))),
            Err(error) => {
                if let NativeError::Unwind(state) = &error
                    && let Some(thrown) = state.0.downcast_ref::<crate::exceptions::Thrown>()
                {
                    let text = crate::exception_rendering::describe_chain(&thrown.object, 15);
                    let file = thrown.object.get(&Object::protected_key("file"));
                    let line = thrown.object.get(&Object::protected_key("line"));
                    let location = match (file.as_ref().and_then(Value::str_ref), line) {
                        (Some(file), Some(Value::Int(line))) => {
                            Some(crate::diagnostics::SourceLocation {
                                file: file.to_string_lossy(),
                                line: line.max(0) as usize,
                            })
                        }
                        _ => None,
                    };
                    let previous = std::mem::replace(&mut self.diagnostic_location, location);
                    self.warn(format!("Uncaught {text}\n  thrown"));
                    self.diagnostic_location = previous;
                } else {
                    return Err(error);
                }
                Err(self.fatal_during_exception(INVALID_RESULT))
            }
            Ok(_) => Err(internal_error!(
                "unexpected debug info result after validation"
            )),
        }
    }
}
