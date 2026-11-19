//! The slow operands of specialized calls retain the standard native binder.
//!
//! ```php
//! count($countable);  // invokes the object's PHP method
//! strlen($number);    // coerces weakly or throws in a strict calling file
//! ```

use phera_bytecode::{BuiltinFrame, BuiltinUnary, ConstId, Reg};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Value};

use crate::{Vm, calls::execution_loop::BatchExit};

impl Vm {
    #[inline(never)]
    pub(crate) fn execute_builtin_frame(
        &mut self,
        operation: BuiltinFrame,
        destination: Reg,
    ) -> Result<(), NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("builtin caller frame"))?;
        let value = match operation {
            BuiltinFrame::ArgumentCount => Value::Int(frame.argument_count() as i64),
            BuiltinFrame::ArgumentsFrom(first) => self.argument_array(frame, first as usize)?,
            BuiltinFrame::CalledClass => self.called_class_name()?,
        };
        self.set(destination, value);
        Ok(())
    }

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
            BuiltinUnary::GetClass => {
                if let Value::Object(object) = &value {
                    self.set(destination, Value::from(object.class.name()));
                    return Ok(None);
                }
                "get_class"
            }
            BuiltinUnary::Defined => {
                let name = match value {
                    Value::String(name) => name,
                    Value::Float(_) => {
                        phera_value::to_php_string(&value, &mut phera_value::NoDiagnostics)?
                    }
                    _ => return Err(internal_error!("defined builtin literal")),
                };
                let name = name.to_string_lossy();
                let exists = ["true", "false", "null"]
                    .iter()
                    .any(|special| special.eq_ignore_ascii_case(&name))
                    || self.constants.contains_key(name.as_ref());
                self.set(destination, Value::Bool(exists));
                return Ok(None);
            }
            _ => {
                let result = builtin_value(operation, &value)
                    .ok_or_else(|| internal_error!("invalid builtin operand"))?;
                self.set(destination, result);
                return Ok(None);
            }
        };
        self.call_builtin_fallback(name, [value], destination)
    }

    /// Numeric string keys go through the same normalization as array lookups.
    #[inline(never)]
    pub(crate) fn execute_array_key_exists(
        &mut self,
        key: Reg,
        array: Reg,
        destination: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let key = self.read_local(key);
        let array = self.read_local(array);
        if let Value::Array(array) = &array {
            let normalized = match &key {
                Value::Int(key) => Some(ArrayKey::Int(*key)),
                Value::Bool(key) => Some(ArrayKey::Int(i64::from(*key))),
                Value::String(key) => Some(ArrayKey::from_string(key.clone())),
                _ => None,
            };
            if let Some(key) = normalized {
                self.set(destination, Value::Bool(array.contains_key(&key)));
                return Ok(None);
            }
        }
        self.call_builtin_fallback("array_key_exists", [key, array], destination)
    }

    /// String and integer literals compare without allocating a new haystack.
    #[inline(never)]
    pub(crate) fn execute_in_literal_array(
        &mut self,
        needle: Reg,
        haystack: ConstId,
        strict: bool,
        destination: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let needle = self.read_local(needle);
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("literal search frame"))?;
        let Some(Value::Array(values)) = frame.cache().constants.get(haystack.index()) else {
            return Err(internal_error!("literal search haystack"));
        };
        let found = match &needle {
            Value::String(needle) => values.values().any(|value| {
                matches!(value, Value::String(value) if value.as_bytes() == needle.as_bytes())
            }),
            Value::Int(needle) if strict => values.values().any(|value| {
                matches!(value, Value::Int(value) if value == needle)
            }),
            _ if strict => false,
            _ => {
                return self.call_builtin_fallback(
                    "in_array",
                    [needle, Value::Array(values.clone()), Value::Bool(false)],
                    destination,
                );
            }
        };
        self.set(destination, Value::Bool(found));
        Ok(None)
    }

    #[cold]
    fn call_builtin_fallback<const N: usize>(
        &mut self,
        name: &str,
        arguments: [Value; N],
        destination: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let mut callee = self.resolve_callee(name)?;
        callee.argument_binding = crate::calls::callback_binding::ArgumentBinding::Frameless;
        self.prepare_call(callee)?;
        for (index, value) in arguments.into_iter().enumerate() {
            self.store_sent_value(index, value)?;
        }
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
