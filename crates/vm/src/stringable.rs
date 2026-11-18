//! `echo $object` and `(string) $object` run its linked __toString method.
//! The value layer keeps handling scalar conversions and their diagnostics.

use phera_bytecode::{BinaryOp, Reg, Type, TypeMask};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{PhpString, StringComparisonStep, Value, to_php_string};

use crate::Vm;
use crate::string_comparison::{begin_comparison, comparison_value, needs_comparison_step};
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
        } else if needs_comparison_step(op, &left, &right) {
            let step = self.step_comparison(|diagnostics, objects| {
                begin_comparison(op, &left, &right, diagnostics, objects)
            })?;
            let order = match step {
                StringComparisonStep::Complete(order) => order,
                StringComparisonStep::Cast(cast) => self.finish_string_comparison(cast).await?,
            };
            return Ok(comparison_value(op, order));
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
