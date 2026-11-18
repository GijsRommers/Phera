//! `[new Text] == ['text']` resumes an array comparison after __toString.
//! PHP code runs between steps, without a borrow into a compared value.

use phera_bytecode::{BinaryOp, Reg};
use phera_extension_api::{LocalFuture, NativeError};
use phera_value::{
    Diagnostics, ObjectComparator, OpError, StringComparison, StringComparisonStep, Value,
};

use crate::Vm;

pub(crate) struct PendingComparison {
    pub(crate) cast: Box<StringComparison>,
    pub(crate) operator: BinaryOp,
    pub(crate) destination: Reg,
}

/// Only these pairs can reach a PHP object-to-string conversion.
#[inline]
pub(crate) fn needs_comparison_step(operator: BinaryOp, left: &Value, right: &Value) -> bool {
    matches!(
        (left, right),
        (Value::Object(_), Value::Object(_) | Value::String(_))
            | (Value::String(_), Value::Object(_))
            | (Value::Array(_), Value::Array(_))
    ) && crate::object_comparison::is_comparison(operator)
}

pub(crate) fn begin_comparison(
    operator: BinaryOp,
    left: &Value,
    right: &Value,
    diagnostics: &mut dyn Diagnostics,
    objects: &mut ObjectComparator<'_>,
) -> Result<StringComparisonStep, OpError> {
    let (left, right) = match operator {
        BinaryOp::Greater | BinaryOp::GreaterEqual => (right, left),
        _ => (left, right),
    };
    phera_value::compare_with_string_objects(left, right, objects, diagnostics)
}

pub(crate) fn comparison_value(operator: BinaryOp, order: i8) -> Value {
    match operator {
        BinaryOp::Equal => Value::Bool(order == 0),
        BinaryOp::NotEqual => Value::Bool(order != 0),
        BinaryOp::Less | BinaryOp::Greater => Value::Bool(order < 0),
        BinaryOp::LessEqual | BinaryOp::GreaterEqual => Value::Bool(order <= 0),
        _ => Value::Int(i64::from(order)),
    }
}

impl Vm {
    #[cold]
    pub(crate) fn finish_string_comparison(
        &mut self,
        mut cast: Box<StringComparison>,
    ) -> LocalFuture<'_, Result<i8, NativeError>> {
        Box::pin(async move {
            loop {
                if !self.error_handlers.pending.is_empty() {
                    self.drain_error_handlers().await?;
                }
                let object = Value::Object(cast.object().clone());
                let string = self.object_to_string(&object).await?;
                let next = self.step_comparison(|diagnostics, objects| {
                    cast.resume(string, objects, diagnostics)
                })?;
                match next {
                    StringComparisonStep::Complete(order) => return Ok(order),
                    StringComparisonStep::Cast(next) => cast = next,
                }
            }
        })
    }
}
