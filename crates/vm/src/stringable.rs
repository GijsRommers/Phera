//! `echo $object` and `(string) $object` run its linked __toString method.
//! The value layer keeps handling scalar conversions and their diagnostics.

use phera_bytecode::{BinaryOp, Reg, Type, TypeMask};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{PhpString, Value, to_php_string};

use crate::Vm;
use crate::types::TypeChecking;

impl Vm {
    pub(crate) fn has_string_conversion(value: &Value) -> bool {
        let Value::Object(object) = &*value.deref() else {
            return false;
        };
        object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
            .and_then(|class| class.magic.as_ref())
            .is_some_and(|magic| magic.to_string.is_some())
    }

    pub(crate) async fn object_to_string(
        &mut self,
        value: &Value,
    ) -> Result<Option<PhpString>, NativeError> {
        let Value::Object(object) = value.deref().clone() else {
            return Ok(None);
        };
        let Some(class) = self.classes.for_object(&object) else {
            return Ok(None);
        };
        let Some(method) = class
            .magic
            .as_ref()
            .and_then(|magic| magic.to_string.clone())
        else {
            return Ok(None);
        };
        let result = self
            .invoke_magic_method(method, class.clone(), object, [])
            .await?;
        if let Value::String(text) = result.deref().clone() {
            return Ok(Some(text));
        }
        Err(NativeError::error(format!(
            "Method {}::__toString() must return a string value",
            class.name
        )))
    }

    pub(crate) async fn stringify_value(
        &mut self,
        value: &Value,
    ) -> Result<PhpString, NativeError> {
        if let Some(text) = self.object_to_string(value).await? {
            return Ok(text);
        }
        Ok(to_php_string(value, self)?)
    }

    pub(crate) fn needs_string_coercion(
        &self,
        value: &Value,
        ty: &Type,
        mode: TypeChecking,
    ) -> bool {
        mode == TypeChecking::Coerce
            && ty.mask.intersects(TypeMask::STRING)
            && !ty.mask.intersects(TypeMask::OBJECT)
            && Self::has_string_conversion(value)
            && !self.matches_class_type(value, ty)
    }

    pub(crate) async fn coerce_stringable(&mut self, value: Value) -> NativeResult {
        let Some(string) = self.object_to_string(&value).await? else {
            return Ok(value);
        };
        if let Value::Reference(cell) = &value {
            // Only retain the cell, not a borrow, while the method is running.
            *cell
                .try_borrow_mut()
                .map_err(|_| internal_error!("stringable reference"))? = Value::String(string);
            Ok(value)
        } else {
            Ok(Value::String(string))
        }
    }

    pub(crate) fn concat_uses_object(&self, left: Reg, right: Reg) -> bool {
        self.operand(left).is_object() || self.operand(right).is_object()
    }

    pub(crate) async fn binary_with_strings(
        &mut self,
        op: BinaryOp,
        mut left: Value,
        mut right: Value,
    ) -> NativeResult {
        if op == BinaryOp::Concat {
            let lhs = self.stringify_value(&left).await?;
            let rhs = self.stringify_value(&right).await?;
            left = Value::String(lhs);
            right = Value::String(rhs);
        } else if is_loose_comparison(op) && mixed_string_comparison(&left, &right) {
            if left.is_object() {
                left = self.coerce_stringable(left).await?;
            } else {
                right = self.coerce_stringable(right).await?;
            }
        }
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        Ok(crate::ops::binary(
            op,
            &left,
            &right,
            &mut diagnostics.buffer,
        )?)
    }
}

#[inline]
pub(crate) fn needs_binary_cast(op: BinaryOp, left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Object(_), Value::String(_)) | (Value::String(_), Value::Object(_)) => {
            op == BinaryOp::Concat || is_loose_comparison(op)
        }
        (Value::Object(_), _) | (_, Value::Object(_)) => op == BinaryOp::Concat,
        _ => false,
    }
}

fn mixed_string_comparison(left: &Value, right: &Value) -> bool {
    matches!(
        (&*left.deref(), &*right.deref()),
        (Value::Object(_), Value::String(_)) | (Value::String(_), Value::Object(_))
    )
}

fn is_loose_comparison(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
            | BinaryOp::Spaceship
    )
}
