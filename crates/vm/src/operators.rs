//! Borrowed arithmetic operands and amortised in-place string assignment.
use crate::{Vm, ops};
use phera_bytecode::{BinaryOp, CastKind, IncDec, Reg, UnaryOp};
use phera_extension_api::NativeError;
use phera_value::{Value, to_php_string};
use std::rc::Rc;

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
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let result = ops::binary(operator, &left, &Value::Int(right as i64), &mut diagnostics);
        self.diagnostics_flush_reuse(&mut diagnostics);
        self.diagnostic_reporter.operator_diagnostics = diagnostics;
        self.set(destination, result?);
        Ok(())
    }

    pub(crate) fn execute_binary(
        &mut self,
        operator: BinaryOp,
        destination: Reg,
        left: Reg,
        right: Reg,
    ) -> Result<(), NativeError> {
        let frame = self
            .call_stack
            .frames
            .last()
            .ok_or_else(|| NativeError::error("Phera internal error: operator frame"))?;
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
                ops::binary(
                    operator,
                    &left,
                    &right,
                    &mut self.diagnostic_reporter.operator_diagnostics,
                )
            });
        drop(left);
        drop(right);
        if !self
            .diagnostic_reporter
            .operator_diagnostics
            .entries
            .is_empty()
        {
            let mut diagnostics =
                std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
            for (kind, message) in diagnostics.entries.drain(..) {
                let kind = match kind {
                    phera_value::DiagnosticKind::Warning => "Warning",
                    phera_value::DiagnosticKind::Notice => "Notice",
                    phera_value::DiagnosticKind::Deprecated => "Deprecated",
                };
                self.diagnostic(kind, message);
            }
            self.diagnostic_reporter.operator_diagnostics = diagnostics;
        }
        self.set(destination, result?);
        Ok(())
    }

    pub(crate) fn assign_concat(&mut self, local: Reg, source: Reg) -> Result<bool, NativeError> {
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let source = to_php_string(&self.operand(source), &mut diagnostics)?;
        let target = self
            .call_stack
            .frames
            .last_mut()
            .and_then(|frame| frame.slots_mut(&mut self.registers).get_mut(local.index()))
            .and_then(Option::as_mut);
        let changed = match target {
            Some(Value::String(target)) => {
                target.push_bytes(source.as_bytes());
                true
            }
            Some(Value::Reference(reference)) => {
                let mut target = reference
                    .try_borrow_mut()
                    .map_err(|_| NativeError::error("Phera internal error: borrowed string"))?;
                if let Value::String(target) = &mut *target {
                    target.push_bytes(source.as_bytes());
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        self.diagnostics_flush_reuse(&mut diagnostics);
        self.diagnostic_reporter.operator_diagnostics = diagnostics;
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
                .unwrap_or(Value::Float(*left as f64 + *right as f64)),
            Subtract => left
                .checked_sub(*right)
                .map(Value::Int)
                .unwrap_or(Value::Float(*left as f64 - *right as f64)),
            Multiply => left
                .checked_mul(*right)
                .map(Value::Int)
                .unwrap_or(Value::Float(*left as f64 * *right as f64)),
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
    pub(crate) fn execute_cast(
        &mut self,
        to: phera_bytecode::CastKind,
        dst: Reg,
        src: Reg,
    ) -> Result<(), NativeError> {
        let value = self.get(src);
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let result = match to {
            CastKind::Bool => Value::Bool(phera_value::cast_to_bool(&value, &mut diagnostics)),
            CastKind::Int => Value::Int(phera_value::to_int(&value, &mut diagnostics)?),
            CastKind::Float => Value::Float(phera_value::to_float(&value, &mut diagnostics)?),
            CastKind::String => Value::String(to_php_string(&value, &mut diagnostics)?),
            CastKind::Array => {
                Value::Array(Rc::new(phera_value::to_array(&value, &mut diagnostics)))
            }
            CastKind::Void => Value::Null,
            CastKind::Object => {
                return Err(NativeError::error("Object casts not supported yet"));
            }
        };
        self.diagnostics_flush(diagnostics);
        self.set(dst, result);

        Ok(())
    }
}

impl Vm {
    pub(crate) fn execute_inc_dec(
        &mut self,
        mode: phera_bytecode::IncDec,
        dst: Reg,
        local: Reg,
    ) -> Result<(), NativeError> {
        let old = self.read_local(local);
        let increment = matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement);
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let value = if let Value::Int(integer) = old {
            let step = if increment { 1 } else { -1 };
            Ok(integer
                .checked_add(step)
                .map(Value::Int)
                .unwrap_or(Value::Float(integer as f64 + step as f64)))
        } else if increment {
            phera_value::increment(&old, &mut diagnostics)
        } else {
            phera_value::decrement(&old, &mut diagnostics)
        };
        self.diagnostics_flush(diagnostics);
        let value = value?;
        self.assign_local(local, value.clone())?;
        self.set(
            dst,
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
        op: BinaryOp,
        local: Reg,
        src: Reg,
    ) -> Result<(), NativeError> {
        // A compiler-elided local snapshot must retain its undefined-variable notice.
        if self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(src.index()))
            .is_some_and(Option::is_none)
        {
            self.read_local(src);
        }
        let reference = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(local.index()))
            .and_then(Option::as_ref)
            .is_some_and(Value::is_reference);
        if op != phera_bytecode::BinaryOp::Concat && !reference {
            self.execute_binary(op, local, local, src)?;
            return Ok(());
        }
        if op == phera_bytecode::BinaryOp::Concat && self.assign_concat(local, src)? {
            return Ok(());
        }
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let value = ops::binary(
            op,
            &self.operand(local),
            &self.operand(src),
            &mut diagnostics,
        );
        self.diagnostics_flush(diagnostics);
        self.assign_local(local, value?)?;
        Ok(())
    }
    /// Implements PHP Unary semantics outside the dispatch loop.
    pub(crate) fn execute_unary(
        &mut self,
        dst: Reg,
        op: UnaryOp,
        src: Reg,
    ) -> Result<(), NativeError> {
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let value = ops::unary(op, &self.operand(src), &mut diagnostics);
        self.diagnostics_flush(diagnostics);
        self.set(dst, value?);
        Ok(())
    }
}
