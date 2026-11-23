//! `[new Text] == ['text']` resumes an array comparison after __toString.
//! PHP code runs between steps, without a borrow into a compared value.

use phera_bytecode::{BinaryOp, Reg};
use phera_extension_api::{LocalFuture, NativeError};
use phera_value::{Diagnostics, ObjectComparator, OpError, StringComparisonStep, Value};

use crate::Vm;

pub(crate) struct PendingComparison {
    pub(crate) work: StringComparisonStep,
    pub(crate) operator: BinaryOp,
    pub(crate) destination: Reg,
}

/// Only these pairs can reach a PHP object-to-string conversion.
#[inline]
pub(crate) fn needs_comparison_step(operator: BinaryOp, left: &Value, right: &Value) -> bool {
    matches!(
        (left, right),
        (
            Value::Object(_),
            Value::Object(_) | Value::InternedStr(_) | Value::String(_)
        ) | (Value::InternedStr(_) | Value::String(_), Value::Object(_))
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
    pub(crate) fn finish_comparison_in_callback(
        &mut self,
        mut work: StringComparisonStep,
    ) -> LocalFuture<'_, Result<i8, NativeError>> {
        Box::pin(async move {
            loop {
                work = match work {
                    StringComparisonStep::Complete(order) => return Ok(order),
                    StringComparisonStep::Cast(cast) => {
                        if !self.error_handlers.pending.is_empty() {
                            self.drain_error_handlers().await?;
                        }
                        let string = self
                            .object_to_string(&Value::Object(cast.object().clone()))
                            .await?;
                        self.step_comparison(|diagnostics, objects| {
                            cast.resume(string, objects, diagnostics)
                        })?
                    }
                    StringComparisonStep::Initialize(comparison) => {
                        let object = comparison
                            .pending()
                            .ok_or_else(|| internal_error!("lazy comparison receiver"))?;
                        self.initialize_lazy_via_callback(object).await?;
                        self.step_comparison(|diagnostics, objects| {
                            comparison.resume(objects, diagnostics)
                        })?
                    }
                };
            }
        })
    }

    pub(crate) async fn continue_comparison(
        &mut self,
        mut pending: PendingComparison,
    ) -> Result<(), NativeError> {
        loop {
            pending.work = match pending.work {
                StringComparisonStep::Complete(order) => {
                    self.set(
                        pending.destination,
                        comparison_value(pending.operator, order),
                    );
                    return Ok(());
                }
                StringComparisonStep::Cast(cast) => {
                    if !self.error_handlers.pending.is_empty() {
                        self.drain_error_handlers().await?;
                    }
                    let string = self
                        .object_to_string(&Value::Object(cast.object().clone()))
                        .await?;
                    self.step_comparison(|diagnostics, objects| {
                        cast.resume(string, objects, diagnostics)
                    })?
                }
                StringComparisonStep::Initialize(comparison) => {
                    let object = comparison
                        .pending()
                        .ok_or_else(|| internal_error!("lazy comparison receiver"))?;
                    let continuation = crate::lazy_continuation::LazyContinuation::Comparison(
                        pending.destination,
                        pending.operator,
                        comparison,
                    );
                    self.suspend_lazy_initialization(object, continuation)
                        .await?;
                    return Ok(());
                }
            };
        }
    }
}
