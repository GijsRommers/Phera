//! Borrowed arithmetic operands and amortised in-place string assignment.

use crate::calls::execution_loop::BatchExit;
use phera_bytecode::{
    BinaryOp, CastKind, ConstId, IncDec, Instruction, Reg, StrictComparison, UnaryOp,
};
use phera_extension_api::{Context, NativeError, PropertyPurpose};
use phera_value::{StringComparisonStep, Value, to_php_string};

use crate::integer_arithmetic::{OverflowOperation, overflow_value};
use crate::string_comparison::{
    PendingComparison, begin_comparison, comparison_value, needs_comparison_step,
};
use crate::{Vm, ops};

impl Vm {
    /// Compare against a borrowed table literal without populating another slot.
    pub(crate) fn execute_strict_compare_const(
        &mut self,
        dst: Reg,
        lhs: Reg,
        rhs: ConstId,
        comparison: StrictComparison,
    ) -> Result<Option<BatchExit>, NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("operator frame"))?;
        let Some(Some(left)) = frame.slots(&self.registers).get(lhs.index()) else {
            return Ok(Some(BatchExit::Instruction(
                Instruction::StrictCompareConst {
                    dst,
                    lhs,
                    rhs,
                    comparison,
                },
            )));
        };
        let right = frame
            .cache()
            .constants
            .get(rhs.index())
            .ok_or_else(|| internal_error!("constant"))?;
        let value = strict_compare_const(left, right, comparison)?;
        self.set(dst, value);
        Ok(None)
    }

    pub(crate) async fn strict_compare_const_with_undefined_operand(
        &mut self,
        dst: Reg,
        lhs: Reg,
        rhs: ConstId,
        comparison: StrictComparison,
    ) -> Result<(), NativeError> {
        let missing = self.warn_missing_operand(lhs).await?;
        let left = if missing { Value::Null } else { self.get(lhs) };
        let right = self.literal_constant(rhs)?;
        let value = strict_compare_const(&left, right, comparison)?;
        self.set(dst, value);
        Ok(())
    }

    /// Read a local before applying the literal, retaining undefined-variable warnings.
    pub(crate) fn execute_binary_int(
        &mut self,
        operator: BinaryOp,
        destination: Reg,
        left: Reg,
        right: i32,
    ) -> Result<(), NativeError> {
        let left = self.read_local(left);
        if left.is_object()
            && let Some(value) =
                self.native_binary(operator, &left, &Value::Int(i64::from(right)))?
        {
            self.set(destination, value);
            return Ok(());
        }
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let result = ops::binary(
            operator,
            &left,
            &Value::Int(right as i64),
            &mut diagnostics.buffer,
        );
        drop(diagnostics);
        self.set(destination, result?);
        Ok(())
    }

    /// Borrow operands so arithmetic does not add array or string owners.
    pub(crate) fn execute_binary(
        &mut self,
        operator: BinaryOp,
        destination: Reg,
        lhs: Reg,
        rhs: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("operator frame"))?;
        let left = frame
            .slots(&self.registers)
            .get(lhs.index())
            .and_then(Option::as_ref);
        let right = frame
            .slots(&self.registers)
            .get(rhs.index())
            .and_then(Option::as_ref);
        let (Some(left), Some(right)) = (left, right) else {
            return Ok(Some(BatchExit::UndefinedBinary {
                dst: destination,
                op: operator,
                lhs,
                rhs,
            }));
        };
        let left = left.deref();
        let right = right.deref();
        let result = match fast_binary(operator, &left, &right) {
            None if crate::object_operators::has_binary(&left, &right) => {
                return Ok(Some(BatchExit::Instruction(Instruction::Binary {
                    dst: destination,
                    op: operator,
                    lhs,
                    rhs,
                })));
            }
            Some(value) => Ok(value),
            None if needs_comparison_step(operator, &left, &right) => {
                if self.classes.has_property_views
                    && matches!(
                        (&*left, &*right),
                        (Value::Array(_), Value::Array(_)) | (Value::Object(_), Value::Object(_))
                    )
                {
                    let (a, b) = ((*left).clone(), (*right).clone());
                    drop(left);
                    drop(right);
                    let step = self.step_comparison(|diagnostics, objects| {
                        begin_comparison(operator, &a, &b, diagnostics, objects)
                    })?;
                    return Ok(self.store_comparison(step, operator, destination));
                }
                let step = begin_comparison(
                    operator,
                    &left,
                    &right,
                    self.diagnostic_reporter.operator(),
                    &mut crate::object_comparison::compare_objects_without_context,
                );
                match step {
                    Ok(StringComparisonStep::Complete(order)) => {
                        Ok(comparison_value(operator, order))
                    }
                    Ok(
                        step
                        @ (StringComparisonStep::Cast(_) | StringComparisonStep::Initialize(_)),
                    ) => {
                        drop(left);
                        drop(right);
                        if self.diagnostic_reporter.has_pending() {
                            self.flush_operator_diagnostics();
                        }
                        return Ok(self.store_comparison(step, operator, destination));
                    }
                    Err(error) => Err(error),
                }
            }
            None => {
                if operator == BinaryOp::Concat && (left.is_object() || right.is_object()) {
                    return Ok(Some(BatchExit::Instruction(Instruction::Binary {
                        dst: destination,
                        op: operator,
                        lhs,
                        rhs,
                    })));
                }
                ops::binary(operator, &left, &right, self.diagnostic_reporter.operator())
            }
        };
        drop(left);
        drop(right);
        if self.diagnostic_reporter.has_pending() {
            self.flush_operator_diagnostics();
        }
        self.set(destination, result?);
        Ok(None)
    }

    pub(crate) async fn binary_with_undefined_operand(
        &mut self,
        op: BinaryOp,
        destination: Reg,
        lhs: Reg,
        rhs: Reg,
    ) -> Result<(), NativeError> {
        // Zend emits `right < left` for `left > right` after compiling both
        // operand expressions in source order.
        let (left_missing, right_missing) =
            if matches!(op, BinaryOp::Greater | BinaryOp::GreaterEqual) {
                let right = self.warn_missing_operand(rhs).await?;
                (self.warn_missing_operand(lhs).await?, right)
            } else {
                let left = self.warn_missing_operand(lhs).await?;
                (left, self.warn_missing_operand(rhs).await?)
            };
        let left = if left_missing {
            Value::Null
        } else {
            self.get(lhs)
        };
        let right = if right_missing {
            Value::Null
        } else {
            self.get(rhs)
        };
        let value = self.binary_with_strings(op, left, right).await?;
        self.set(destination, value);
        Ok(())
    }

    /// Append directly to strings, including referenced strings, to retain amortized growth.
    pub(crate) fn assign_concat(&mut self, local: Reg, source: Reg) -> Result<bool, NativeError> {
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let source = to_php_string(&diagnostics.vm.operand(source), &mut diagnostics.buffer)?;
        let target = diagnostics
            .vm
            .current_slot_mut(local)
            .and_then(Option::as_mut);
        let changed = match target {
            Some(Value::String(target)) => {
                target.push_bytes(source.as_bytes());
                true
            }
            Some(Value::Reference(reference)) => {
                let mut target = reference
                    .try_borrow_mut()
                    .map_err(|_| internal_error!("borrowed string"))?;
                if let Value::String(target) = &mut *target {
                    target.push_bytes(source.as_bytes());
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        drop(diagnostics);
        Ok(changed)
    }
}

/// A string literal cannot trigger object conversions or recursive comparisons.
pub(crate) fn strict_compare_const(
    left: &Value,
    right: &Value,
    comparison: StrictComparison,
) -> Result<Value, NativeError> {
    let equal = phera_value::strict_equals(left, right, &mut phera_value::NoDiagnostics)?;
    let equal = matches!(equal, Value::Bool(true));
    Ok(Value::Bool(match comparison {
        StrictComparison::Identical => equal,
        StrictComparison::NotIdentical => !equal,
    }))
}

/// The common scalar cases do not need PHP's general coercion machinery.
#[inline(always)]
pub(crate) fn fast_binary(operator: BinaryOp, left: &Value, right: &Value) -> Option<Value> {
    use BinaryOp::{
        Add, Equal, Greater, GreaterEqual, Identical, Less, LessEqual, Modulo, Multiply, NotEqual,
        NotIdentical, Subtract,
    };
    match (left, right) {
        (Value::Int(left), Value::Int(right)) => Some(match operator {
            Add => left
                .checked_add(*right)
                .map(Value::Int)
                .unwrap_or_else(|| overflow_value(OverflowOperation::Add, *left, *right)),
            Subtract => left
                .checked_sub(*right)
                .map(Value::Int)
                .unwrap_or_else(|| overflow_value(OverflowOperation::Subtract, *left, *right)),
            Multiply => left
                .checked_mul(*right)
                .map(Value::Int)
                .unwrap_or_else(|| overflow_value(OverflowOperation::Multiply, *left, *right)),
            Modulo if *right != 0 => Value::Int(if *right == -1 { 0 } else { left % right }),
            Less => Value::Bool(left < right),
            LessEqual => Value::Bool(left <= right),
            Greater => Value::Bool(left > right),
            GreaterEqual => Value::Bool(left >= right),
            Equal | Identical => Value::Bool(left == right),
            NotEqual | NotIdentical => Value::Bool(left != right),
            _ => return None,
        }),
        (Value::Float(left), Value::Float(right)) => Some(match operator {
            Add => Value::Float(left + right),
            Subtract => Value::Float(left - right),
            Multiply => Value::Float(left * right),
            Less => Value::Bool(left < right),
            LessEqual => Value::Bool(left <= right),
            Greater => Value::Bool(left > right),
            GreaterEqual => Value::Bool(left >= right),
            _ => return None,
        }),
        _ => None,
    }
}

impl Vm {
    /// Apply PHP conversions and emit their diagnostics before publishing the result.
    fn store_comparison(
        &mut self,
        step: StringComparisonStep,
        operator: BinaryOp,
        destination: Reg,
    ) -> Option<BatchExit> {
        match step {
            StringComparisonStep::Complete(order) => {
                self.set(destination, comparison_value(operator, order));
                None
            }
            work => Some(BatchExit::Compare(PendingComparison {
                work,
                operator,
                destination,
            })),
        }
    }

    pub(crate) fn execute_cast(
        &mut self,
        to: phera_bytecode::CastKind,
        destination: Reg,
        source: Reg,
    ) -> Result<(), NativeError> {
        let value = self.get(source);
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let result = match to {
            CastKind::Bool => {
                Value::Bool(phera_value::cast_to_bool(&value, &mut diagnostics.buffer))
            }
            CastKind::Int => Value::Int(phera_value::to_int(&value, &mut diagnostics.buffer)?),
            CastKind::Float => {
                Value::Float(phera_value::to_float(&value, &mut diagnostics.buffer)?)
            }
            CastKind::String => Value::String(to_php_string(&value, &mut diagnostics.buffer)?),
            CastKind::Array => Value::from(match &value {
                Value::Object(object) => diagnostics
                    .vm
                    .object_properties(object, PropertyPurpose::ArrayCast)?,
                _ => phera_value::to_array(&value, &mut diagnostics.buffer),
            }),
            CastKind::Void => Value::Null,
            CastKind::Object => {
                if value.is_object() {
                    value
                } else {
                    let class = diagnostics
                        .vm
                        .classes
                        .get("stdClass")
                        .ok_or_else(|| internal_error!("stdClass"))?;
                    let object = diagnostics.vm.classes.allocate(class);
                    match value {
                        Value::Null => {}
                        Value::Array(array) => {
                            for (key, value) in array.iter() {
                                let key = match key {
                                    phera_value::ArrayKey::Int(index) => {
                                        phera_value::ArrayKey::str(index.to_string())
                                    }
                                    key => key,
                                };
                                object.set(key, value.clone());
                            }
                        }
                        value => {
                            object.set(phera_value::ArrayKey::str("scalar"), value);
                        }
                    }
                    Value::Object(object)
                }
            }
        };
        drop(diagnostics);
        self.set(destination, result);

        Ok(())
    }
}

impl Vm {
    /// Preserve pre/post expression values while assigning through local references.
    pub(crate) fn execute_inc_dec(
        &mut self,
        mode: phera_bytecode::IncDec,
        destination: Reg,
        local: Reg,
    ) -> Result<(), NativeError> {
        let old = self.read_local(local);
        let increment = matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement);
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let native = if old.is_object() {
            let operator = if increment {
                BinaryOp::Add
            } else {
                BinaryOp::Subtract
            };
            diagnostics
                .vm
                .native_binary(operator, &old, &Value::Int(1))?
        } else {
            None
        };
        let value = if let Some(value) = native {
            Ok(value)
        } else if let Value::Int(integer) = old {
            let step = if increment { 1 } else { -1 };
            Ok(integer
                .checked_add(step)
                .map(Value::Int)
                .unwrap_or(Value::Float(integer as f64 + step as f64)))
        } else if increment {
            phera_value::increment(&old, &mut diagnostics.buffer)
        } else {
            phera_value::decrement(&old, &mut diagnostics.buffer)
        };
        drop(diagnostics);
        let value = value?;
        self.assign_local(local, value.clone())?;
        self.set(
            destination,
            if matches!(mode, IncDec::PostIncrement | IncDec::PostDecrement) {
                old
            } else {
                value
            },
        );

        Ok(())
    }
}

impl Vm {
    /// Implements PHP AssignOp semantics outside the dispatch loop.
    pub(crate) fn execute_assign_op(
        &mut self,
        operator: BinaryOp,
        local: Reg,
        source: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let unassigned = |vm: &Self, register: Reg| {
            vm.current()
                .and_then(|frame| frame.slots(&vm.registers).get(register.index()))
                .is_some_and(Option::is_none)
        };
        // `$missing += 1` reads a variable that was never assigned. The
        // slow path warns for it and lets an error handler run before the
        // result is stored, as PHP does.
        if unassigned(self, local) {
            return Ok(Some(BatchExit::Instruction(Instruction::AssignOp {
                op: operator,
                local,
                src: source,
            })));
        }
        // A compiler-elided local snapshot must retain its undefined-variable notice.
        if unassigned(self, source) {
            self.read_local(source);
        }
        let reference = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(local.index()))
            .and_then(Option::as_ref)
            .is_some_and(Value::is_reference);
        if operator != phera_bytecode::BinaryOp::Concat && !reference {
            return self.execute_binary(operator, local, local, source);
        }
        if operator == BinaryOp::Concat {
            if self.concat_uses_object(local, source) {
                return Ok(Some(BatchExit::Instruction(Instruction::AssignOp {
                    op: operator,
                    local,
                    src: source,
                })));
            }
            if self.assign_concat(local, source)? {
                return Ok(None);
            }
        }
        // Only an object operand can overload the operator; the common case
        // keeps borrowing its operands.
        if self.operand(local).is_object() || self.operand(source).is_object() {
            let left = (*self.operand(local)).clone();
            let right = (*self.operand(source)).clone();
            if let Some(value) = self.native_binary(operator, &left, &right)? {
                self.assign_local(local, value)?;
                return Ok(None);
            }
        }
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let value = ops::binary(
            operator,
            &diagnostics.vm.operand(local),
            &diagnostics.vm.operand(source),
            &mut diagnostics.buffer,
        );
        drop(diagnostics);
        self.assign_local(local, value?)?;
        Ok(None)
    }

    /// Implements PHP Unary semantics outside the dispatch loop.
    pub(crate) fn execute_unary(
        &mut self,
        destination: Reg,
        operator: UnaryOp,
        source: Reg,
    ) -> Result<(), NativeError> {
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let source_value = diagnostics.vm.get(source);
        if let Some(value) = diagnostics.vm.native_unary(operator, &source_value)? {
            drop(diagnostics);
            self.set(destination, value);
            return Ok(());
        }
        let value = ops::unary(
            operator,
            &diagnostics.vm.operand(source),
            &mut diagnostics.buffer,
        );
        drop(diagnostics);
        self.set(destination, value?);
        Ok(())
    }
}
