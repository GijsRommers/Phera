//! Every evaluated callback uses one resolver before argument binding or native dispatch.

use crate::{Vm, call_state::Callee, classes::Invocation};
use phera_bytecode::ClassBinding;
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Value};
use std::rc::Rc;

impl Vm {
    /// Strings name functions or static methods; arrays bind an object or name a class.
    pub(crate) fn resolve_callable(&mut self, value: &Value) -> Result<Rc<Callee>, NativeError> {
        match &*value.deref() {
            Value::String(name) => {
                let name = name.to_string_lossy();
                if let Some((class, method)) = name.split_once("::") {
                    self.prepare_static_style(class, method, ClassBinding::Explicit)?;
                    self.take_resolved_callable()
                } else {
                    self.resolve_callee(name)
                }
            }
            Value::Array(array) => {
                if array.len() != 2 {
                    return Err(NativeError::error(
                        "Array callback must have exactly two elements",
                    ));
                }
                let target = array.get(&ArrayKey::Int(0)).ok_or_else(|| {
                    NativeError::error("Array callback has to contain indices 0 and 1")
                })?;
                let method = array.get(&ArrayKey::Int(1)).ok_or_else(|| {
                    NativeError::error("Array callback has to contain indices 0 and 1")
                })?;
                let Value::String(method) = &*method.deref() else {
                    return Err(NativeError::error("Method name must be a string"));
                };
                let method = method.to_string_lossy();
                match &*target.deref() {
                    Value::String(class) => self.prepare_static_style(
                        &class.to_string_lossy(),
                        &method,
                        ClassBinding::Explicit,
                    )?,
                    Value::Object(object) => {
                        let class = self
                            .classes
                            .get(object.class.name())
                            .ok_or_else(|| internal_error!("callback class"))?;
                        self.prepare_method(
                            class,
                            Some(object.clone()),
                            &method,
                            Invocation::Method,
                        )?;
                    }
                    _ => {
                        return Err(NativeError::error(
                            "First array member is not a valid class name or object",
                        ));
                    }
                }
                self.take_resolved_callable()
            }
            _ => Err(NativeError::error(format!(
                "Value of type {} is not callable",
                value.type_name()
            ))),
        }
    }

    /// Method preparation snapshots its scope in the existing pending-call representation.
    fn take_resolved_callable(&mut self) -> Result<Rc<Callee>, NativeError> {
        self.call_stack
            .current_mut()
            .and_then(|frame| frame.calls.pop())
            .map(|call| call.callee)
            .ok_or_else(|| internal_error!("resolved callable"))
    }
}
