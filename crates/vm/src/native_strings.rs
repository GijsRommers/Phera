//! Native stringable objects support `echo $type` and `(string) $type`.

use std::rc::Rc;

use phera_extension_api::{NativeError, NativeMethod};
use phera_value::{Object, PhpString, Value};

use crate::{Vm, classes::MethodBody};

impl Vm {
    pub(crate) fn native_object_string(
        &mut self,
        object: &Rc<Object>,
    ) -> Result<Option<PhpString>, NativeError> {
        let Some(class) = self.classes.for_object(object) else {
            return Ok(None);
        };
        if class.is_throwable {
            return Ok(match self.throwable_method_result("__toString", object) {
                Some(Value::String(text)) => Some(text),
                _ => None,
            });
        }
        let Some(method) = class.methods.get("__tostring") else {
            return Ok(None);
        };
        let MethodBody::Native(entry) = method.body else {
            return Ok(None);
        };
        let NativeMethod::Sync(string) = entry.handler else {
            return Ok(None);
        };
        match string(self, Some(object), &[])? {
            Value::String(text) => Ok(Some(text)),
            value => Err(NativeError::type_error(format!(
                "{}::__toString(): Return value must be of type string, {} returned",
                method.declaring,
                value.type_name()
            ))),
        }
    }
}
