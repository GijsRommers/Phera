//! The slow operands of specialized calls retain the standard native binder.
//!
//! ```php
//! count($countable);  // invokes the object's PHP method
//! strlen($number);    // coerces weakly or throws in a strict calling file
//! ```

use phera_bytecode::{BuiltinUnary, Reg};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::{Vm, calls::execution_loop::BatchExit};

impl Vm {
    #[cold]
    pub(crate) fn execute_builtin_unary(
        &mut self,
        operation: BuiltinUnary,
        source: Reg,
        destination: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let value = self.read_local(source);
        let name = match operation {
            BuiltinUnary::Strlen => "strlen",
            BuiltinUnary::Count => "count",
            BuiltinUnary::Sizeof => "sizeof",
            _ => {
                let result = builtin_value(operation, &value)
                    .ok_or_else(|| internal_error!("invalid builtin operand"))?;
                self.set(destination, result);
                return Ok(None);
            }
        };
        let callee = self.resolve_callee(name)?;
        self.prepare_call(callee)?;
        self.store_sent_value(0, value)?;
        self.call_native(destination)
    }
}

/// String length and array size need only the operand's representation.
#[inline]
pub(crate) fn builtin_value(operation: BuiltinUnary, value: &Value) -> Option<Value> {
    use BuiltinUnary as B;
    match (operation, value) {
        (_, Value::Reference(_)) => None,
        (B::Strlen, Value::String(string)) => Some(Value::Int(string.len() as i64)),
        (B::Count | B::Sizeof, Value::Array(array)) => Some(Value::Int(array.len() as i64)),
        (B::TypeCheck(mask), value) => {
            Some(Value::Bool(mask.intersects(crate::types::own_type(value))))
        }
        (B::IsResource, value) => Some(Value::Bool(matches!(
            value,
            Value::Resource(resource) if !resource.is_closed()
        ))),
        (B::GetType, value) => Some(Value::from(value.gettype_name())),
        _ => None,
    }
}
