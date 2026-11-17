//! Borrowed arithmetic operands and amortised in-place string assignment.

use std::rc::Rc;

use phera_bytecode::{BinaryOp, CastKind, IncDec, Reg, UnaryOp};
use phera_extension_api::NativeError;
use phera_value::{Value, to_php_string};

use crate::integer_arithmetic::{OverflowOperation, overflow_value};
use crate::{Vm, ops};

impl Vm {
    /// Read a local before applying the literal, retaining undefined-variable warnings.
    pub(crate) fn execute_binary_int(
        &mut self,
        operator: BinaryOp,
        destination: Reg,
        left: Reg,
        right: i32,
    ) -> Result<(), NativeError> {
        let left = self.read_local(left);
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
        left: Reg,
        right: Reg,
    ) -> Result<(), NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("operator frame"))?;
        let left = frame
            .slots(&self.registers)
            .get(left.index())
            .and_then(Option::as_ref);
        let right = frame
            .slots(&self.registers)
            .get(right.index())
            .and_then(Option::as_ref);
        let null = Value::Null;
        let left = left.unwrap_or(&null).deref();
        let right = right.unwrap_or(&null).deref();
        let result = fast_binary(operator, &left, &right)
            .map(Ok)
            .unwrap_or_else(|| {
                ops::binary(operator, &left, &right, self.diagnostic_reporter.operator())
            });
        drop(left);
        drop(right);
        if self.diagnostic_reporter.has_pending() {
            self.flush_operator_diagnostics();
        }
        self.set(destination, result?);
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

/// The common scalar cases do not need PHP's general coercion machinery.
#[inline]
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
    pub(crate) fn execute_cast(
        &mut self,
        to: phera_bytecode::CastKind,
        destination: Reg,
        source: Reg,
    ) -> Result<(), NativeError> {
        let value = self.get(source);
        if to == CastKind::String
            && let Value::Object(object) = &value
            && self
                .classes
                .for_object(object)
                .is_some_and(|class| class.is_throwable)
        {
            let value = self
                .throwable_method_result("__toString", object)
                .unwrap_or_default();
            self.set(destination, value);
            return Ok(());
        }
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
            CastKind::Array => Value::Array(Rc::new(phera_value::to_array(
                &value,
                &mut diagnostics.buffer,
            ))),
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
                                    key => key.clone(),
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
        let value = if let Value::Int(integer) = old {
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
    ) -> Result<(), NativeError> {
        // A compiler-elided local snapshot must retain its undefined-variable notice.
        if self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(source.index()))
            .is_some_and(Option::is_none)
        {
            self.read_local(source);
        }
        let reference = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(local.index()))
            .and_then(Option::as_ref)
            .is_some_and(Value::is_reference);
        if operator != phera_bytecode::BinaryOp::Concat && !reference {
            self.execute_binary(operator, local, local, source)?;
            return Ok(());
        }
        if operator == phera_bytecode::BinaryOp::Concat && self.assign_concat(local, source)? {
            return Ok(());
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
        Ok(())
    }

    /// Implements PHP Unary semantics outside the dispatch loop.
    pub(crate) fn execute_unary(
        &mut self,
        destination: Reg,
        operator: UnaryOp,
        source: Reg,
    ) -> Result<(), NativeError> {
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
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
