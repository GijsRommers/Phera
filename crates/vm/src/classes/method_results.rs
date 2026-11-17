//! Guarded integer leaf methods return without allocating a PHP frame.

use super::{Class, Invocation, Method, MethodBody, MethodContext};
use crate::Vm;
use phera_bytecode::{BinaryOp, FetchMode, Instruction, PathBase, Reg, Step, TypeMask, Visibility};
use phera_value::{ArrayKey, Value};
use std::rc::Rc;

/// Keyed by resolved method and lookup class in MethodSites. Made after method
/// visibility resolution; the immutable method/default layout cannot invalidate
/// it. Actual slot keys, initialized values, references and overflow are still
/// checked on every use, preserving the slow path after object storage changes.
pub(crate) enum MethodResult {
    Integer {
        value: i64,
    },
    Property {
        key: ArrayKey,
        slot: usize,
        addend: i64,
        instructions: u64,
    },
}

/// A leaf property read optionally adds one immediate integer before returning.
struct IntegerTail {
    addend: i64,
    instructions: u64,
}

/// Only parameterless, nonreference bodies with an already matching return type qualify.
pub(crate) fn result_plan(
    class: &Class,
    scope: &Class,
    method: &Method,
) -> Option<Rc<MethodResult>> {
    let MethodBody::User(reference) = &method.body else {
        return None;
    };
    let function = reference.unit.functions.get(reference.index)?;
    if !function.parameters.is_empty()
        || function.returns_reference
        || function.observes_arguments
        || !function.handlers.is_empty()
    {
        return None;
    }
    if let Some(index) = function.return_type {
        let mask = reference.unit.types.get(index.index())?.mask;
        if mask != TypeMask::MIXED && !mask.intersects(TypeMask::INT) {
            return None;
        }
    }
    let first = function.instructions.first()?;
    let plan = match first {
        Instruction::LoadInt { dst, value } if matches!(function.instructions.get(1), Some(Instruction::Return { src }) if src == dst) => {
            MethodResult::Integer {
                value: (*value).into(),
            }
        }
        Instruction::Fetch {
            dst,
            path,
            mode: FetchMode::Read,
        } if !method.is_static => {
            let path = function.paths.get(path.index())?;
            if !matches!(path.base, PathBase::This { .. }) {
                return None;
            }
            let [Step::Property(name)] = path.steps.as_slice() else {
                return None;
            };
            let name = reference.unit.names.get(name.index())?;
            let tail = integer_tail(&function.instructions[1..], *dst)?;
            let property = class
                .defaults
                .iter()
                .enumerate()
                .find(|(_, property)| {
                    property.name == *name
                        && property.visibility == Visibility::Private
                        && property.declaring == scope.name
                })
                .or_else(|| {
                    class
                        .defaults
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(_, property)| property.name == *name)
                })?;
            if property.1.visibility != Visibility::Public
                && !(property.1.visibility == Visibility::Private
                    && property.1.declaring == scope.name)
            {
                return None;
            }
            MethodResult::Property {
                key: property.1.key.clone(),
                slot: property.0,
                addend: tail.addend,
                instructions: tail.instructions,
            }
        }
        _ => return None,
    };
    Some(Rc::new(plan))
}

/// Match the exact registers involved; arbitrary body instructions always fall back.
fn integer_tail(instructions: &[Instruction], source: Reg) -> Option<IntegerTail> {
    match instructions {
        [Instruction::Return { src }, ..] if *src == source => Some(IntegerTail {
            addend: 0,
            instructions: 2,
        }),
        [
            Instruction::BinaryInt {
                op: BinaryOp::Add,
                dst,
                lhs,
                rhs,
            },
            Instruction::Return { src },
            ..,
        ] if *lhs == source && dst == src => Some(IntegerTail {
            addend: (*rhs).into(),
            instructions: 3,
        }),
        _ => None,
    }
}

impl Vm {
    /// Mirrors push_prepared, the matched integer instructions and return_value.
    /// Missing/aliased properties, overflow, constructors and frame-limit errors
    /// retain the ordinary heap-frame path. No PHP call uses the Rust stack here.
    pub(crate) fn leaf_method_result(&mut self, context: &MethodContext) -> Option<Value> {
        if context.invocation != Invocation::Method {
            return None;
        }
        self.evaluate_method_result(context.result_plan.as_deref()?, context.receiver.as_deref())
    }

    /// Evaluate only the cached infallible integer fragment; failed guards have no side effects.
    pub(crate) fn evaluate_method_result(
        &mut self,
        plan: &MethodResult,
        receiver: Option<&phera_value::Object>,
    ) -> Option<Value> {
        if self.call_stack.depth() >= self.frame_limit {
            return None;
        }
        let value = match plan {
            MethodResult::Integer { value } => {
                self.executed_instructions += 2;
                *value
            }
            MethodResult::Property {
                key,
                slot,
                addend,
                instructions,
            } => {
                let object = receiver?;
                let Value::Int(value) = crate::property_storage::read_at(object, key, Some(*slot))?
                else {
                    return None;
                };
                let value = value.checked_add(*addend)?;
                self.executed_instructions += instructions;
                value
            }
        };
        Some(Value::Int(value))
    }
}
