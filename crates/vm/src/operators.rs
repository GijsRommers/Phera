//! Borrowed arithmetic operands and amortised in-place string assignment.
use crate::{Vm, ops};
use phera_bytecode::{BinaryOp, CastKind, IncDec, Reg};
use phera_extension_api::NativeError;
use phera_value::{CollectDiagnostics, Value, to_php_string};
use std::rc::Rc;

impl Vm {
    pub(crate) fn execute_binary(
        &mut self,
        operator: BinaryOp,
        destination: Reg,
        left: Reg,
        right: Reg,
    ) -> Result<(), NativeError> {
        let frame = self
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
                ops::binary(operator, &left, &right, &mut self.operator_diagnostics)
            });
        drop(left);
        drop(right);
        if !self.operator_diagnostics.entries.is_empty() {
            let mut diagnostics = std::mem::take(&mut self.operator_diagnostics);
            for (kind, message) in diagnostics.entries.drain(..) {
                let kind = match kind {
                    phera_value::DiagnosticKind::Warning => "Warning",
                    phera_value::DiagnosticKind::Notice => "Notice",
                    phera_value::DiagnosticKind::Deprecated => "Deprecated",
                };
                self.diagnostic(kind, message);
            }
            self.operator_diagnostics = diagnostics;
        }
        self.set(destination, result?);
        Ok(())
    }

    pub(crate) fn assign_concat(&mut self, local: Reg, source: Reg) -> Result<bool, NativeError> {
        let mut diagnostics = std::mem::take(&mut self.operator_diagnostics);
        let source = to_php_string(&self.get(source), &mut diagnostics)?;
        let target = self
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
        self.diagnostics_flush(std::mem::take(&mut diagnostics));
        self.operator_diagnostics = diagnostics;
        Ok(changed)
    }
}

/// The common scalar cases do not need PHP's general coercion machinery.
fn fast_binary(operator: BinaryOp, left: &Value, right: &Value) -> Option<Value> {
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
        let mut diagnostics = CollectDiagnostics::default();
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
        let mut diagnostics = CollectDiagnostics::default();
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
