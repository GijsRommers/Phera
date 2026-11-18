//! Native objects compare at any depth in PHP arrays or object properties.
//!
//! ```php
//! [$date] == [$sameInstant]; // the native comparison still applies
//! ```

use phera_bytecode::BinaryOp;
use phera_extension_api::{ClassHooks, Context, NativeComparison, NativeError, PropertyPurpose};
use phera_value::{Diagnostics, Object, ObjectComparison, OpError, Value};

use crate::{Vm, classes::Class};

fn hooks(object: &Object) -> Option<ClassHooks> {
    object
        .class
        .as_any()
        .downcast_ref::<Class>()?
        .hooks
        .as_deref()
        .copied()
}

/// Shared object dispatch for VM operators and the iterative container walker.
pub(crate) fn compare_objects(
    left: &Object,
    right: &Object,
    diagnostics: &mut dyn Diagnostics,
) -> Result<ObjectComparison, OpError> {
    if left.class.name() == "Closure" && right.class.name() == "Closure" {
        return Ok(ObjectComparison::Compared(i8::from(
            !crate::closure_equality::same_callable(left, right),
        )));
    }
    let left_compare = hooks(left).and_then(|hook| hook.compare);
    let right_compare = hooks(right).and_then(|hook| hook.compare);
    let mut result = match left_compare {
        Some(compare) => compare(left, right, diagnostics)?,
        None => NativeComparison::Default,
    };
    if result == NativeComparison::Default
        && let Some(compare) = right_compare
    {
        result = match compare(right, left, diagnostics)? {
            NativeComparison::Ordered(order) => NativeComparison::Ordered(order.reverse()),
            other => other,
        };
    }
    Ok(match result {
        NativeComparison::Default => ObjectComparison::Default,
        NativeComparison::Ordered(order) => ObjectComparison::Compared(order as i8),
        NativeComparison::Uncomparable => ObjectComparison::Compared(1),
    })
}

pub(crate) fn is_comparison(operator: BinaryOp) -> bool {
    matches!(
        operator,
        BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
            | BinaryOp::Spaceship
    )
}

pub(crate) fn compare(
    operator: BinaryOp,
    left: &Value,
    right: &Value,
    diagnostics: &mut dyn Diagnostics,
    objects: &mut phera_value::ObjectComparator<'_>,
) -> Result<Value, OpError> {
    use BinaryOp as Op;
    let (left, right) = if matches!(operator, Op::Greater | Op::GreaterEqual) {
        (right, left)
    } else {
        (left, right)
    };
    let order = phera_value::compare_with_objects(left, right, diagnostics, objects)?;
    Ok(match operator {
        Op::Equal => Value::Bool(order == 0),
        Op::NotEqual => Value::Bool(order != 0),
        Op::Less | Op::Greater => Value::Bool(order < 0),
        Op::LessEqual | Op::GreaterEqual => Value::Bool(order <= 0),
        Op::Spaceship => Value::Int(order.into()),
        _ => return Err(OpError::error("Phera internal error: comparison operator")),
    })
}

impl Vm {
    /// Views may allocate linked objects, so run them only after releasing register borrows.
    pub(crate) fn compare_with_property_views(
        &mut self,
        operator: BinaryOp,
        left: &Value,
        right: &Value,
    ) -> Result<Value, NativeError> {
        let mut diagnostics = self.diagnostic_reporter.take_operator();
        let mut projection_error = None;
        let result = compare(
            operator,
            left,
            right,
            &mut diagnostics,
            &mut |left, right, diagnostics| {
                let result = compare_objects(left, right, diagnostics)?;
                if matches!(result, ObjectComparison::Default)
                    && left.class.name() == right.class.name()
                    && hooks(left).is_some_and(|hooks| hooks.properties.is_some())
                {
                    let views = (|| {
                        Ok(ObjectComparison::Projected(
                            self.object_properties(left, PropertyPurpose::Comparison)?,
                            self.object_properties(right, PropertyPurpose::Comparison)?,
                        ))
                    })();
                    return views.map_err(|error| {
                        projection_error = Some(error);
                        OpError::error("Failed to project native properties")
                    });
                }
                Ok(result)
            },
        );
        self.diagnostics_flush_reuse(&mut diagnostics);
        self.diagnostic_reporter.restore_operator(diagnostics);
        match projection_error {
            Some(error) => Err(error),
            None => Ok(result?),
        }
    }
}
