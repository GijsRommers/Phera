//! `var_dump($object)` and `print_r($object)` show the array returned by
//! __debugInfo. `var_export($object)` exports actual properties as __set_state input.

use std::rc::Rc;

use phera_extension_api::{NativeError, Unwind};
use phera_value::{Object, PhpArray, Value};

use crate::Vm;

impl Vm {
    pub(crate) fn has_debug_info(&self, object: &Object) -> bool {
        object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
            .and_then(|class| class.magic.as_ref())
            .is_some_and(|magic| magic.debug_info.is_some())
    }

    pub(crate) async fn debug_properties(
        &mut self,
        object: Rc<Object>,
    ) -> Result<Rc<PhpArray>, NativeError> {
        let Some(class) = self.classes.for_object(&object) else {
            return Ok(Rc::new(object.to_array()));
        };
        let Some(method) = class
            .magic
            .as_ref()
            .and_then(|magic| magic.debug_info.clone())
        else {
            return Ok(Rc::new(object.to_array()));
        };
        let qualified = format!("{}::{}()", method.declaring, method.name);
        let result = self.invoke_magic_method(method, class, object, []).await;
        match result.map(|value| value.deref().clone()) {
            Ok(Value::Array(properties)) => Ok(properties),
            Ok(Value::Null) => Err(NativeError::error(format!(
                "Returning null from {qualified} is not supported by Phera: deprecated since PHP 8.5, return an empty array instead"
            ))),
            Err(error) => {
                if let NativeError::Unwind(state) = &error
                    && let Some(thrown) = state.0.downcast_ref::<crate::exceptions::Thrown>()
                {
                    let text = crate::exception_rendering::describe_chain(&thrown.object, 15);
                    self.warn(format!("Uncaught {text}\n  thrown"));
                } else {
                    return Err(error);
                }
                Err(self.invalid_debug_info())
            }
            _ => Err(self.invalid_debug_info()),
        }
    }

    fn invalid_debug_info(&self) -> NativeError {
        let location = self.location();
        NativeError::Unwind(Unwind(Box::new(crate::error_traces::DeclarationFailure {
            message: "__debuginfo() must return an array".into(),
            file: location.file,
            line: location.line,
        })))
    }
}
