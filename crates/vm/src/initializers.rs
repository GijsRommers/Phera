//! Pure default-value functions reuse scalar bytecode semantics without invoking PHP callbacks.

use crate::{Vm, code_table::FunctionIndex};
use phera_bytecode::{Instruction, Reg};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, PhpArray, Value};
use std::rc::Rc;

impl Vm {
    /// Initializers have private registers and cannot mutate the caller's locals.
    pub(crate) fn evaluate_initializer(
        &mut self,
        reference: &FunctionIndex,
        scope: &str,
    ) -> NativeResult {
        let previous = self.initializer_scope.replace(scope.to_owned());
        let result = self.evaluate_initializer_body(reference, scope);
        self.initializer_scope = previous;
        result
    }

    /// Only constant-expression instructions may execute in this isolated register window.
    fn evaluate_initializer_body(
        &mut self,
        reference: &FunctionIndex,
        scope: &str,
    ) -> NativeResult {
        // Initializers may call back into PHP code, so this one holds its own
        // owner of the unit rather than a borrow of the code table.
        let unit = self.code.unit(*reference).clone();
        let function = crate::code_table::body(&unit, self.code.local(*reference));
        let mut slots = vec![None; usize::from(function.registers)];
        let mut position = 0;
        let mut prepared = None;
        let mut diagnostics = phera_value::CollectDiagnostics::default();
        while let Some(instruction) = function.instructions.get(position).copied() {
            position += 1;
            if crate::fast::scalar_step(instruction, &mut slots, &mut position)
                == crate::fast::ScalarStep::Complete
            {
                continue;
            }
            match self.initializer_callable(instruction, reference, scope, &mut prepared, &slots)? {
                crate::closure_initializers::InitializerCallable::Prepared => continue,
                crate::closure_initializers::InitializerCallable::Value(destination, value) => {
                    if let Some(slot) = slots.get_mut(destination.index()) {
                        *slot = Some(value);
                    }
                    continue;
                }
                crate::closure_initializers::InitializerCallable::Unrelated => {}
            }
            let (destination, value) = match instruction {
                Instruction::LoadConst { dst, constant } => (
                    dst,
                    crate::unit_caches::value(&unit.constants[constant.index()]),
                ),
                Instruction::FetchClassName { dst, class } => {
                    let name = &unit.symbols[class.index()].written;
                    let name = match name.as_str() {
                        "self" | "static" => scope.to_owned(),
                        "parent" => self
                            .classes
                            .get(scope)
                            .and_then(|class| class.parent.clone())
                            .map(|parent| parent.name.clone())
                            .ok_or_else(|| NativeError::error("Cannot access parent"))?,
                        _ => name.clone(),
                    };
                    (dst, Value::from(name))
                }
                Instruction::FetchClassConstant { dst, class, name } => {
                    let class = slot(&slots, class);
                    let Value::String(class) = class else {
                        return Err(internal_error!("initializer class"));
                    };
                    (
                        dst,
                        self.class_constant(&class.to_string_lossy(), &unit.names[name.index()])?,
                    )
                }
                Instruction::FetchConstant { dst, constant } => {
                    (dst, self.fetch_global_constant(unit.clone(), constant)?)
                }
                Instruction::Binary {
                    dst,
                    op: operator,
                    lhs: left,
                    rhs: right,
                } => (
                    dst,
                    crate::ops::binary(
                        operator,
                        &slot(&slots, left),
                        &slot(&slots, right),
                        &mut diagnostics,
                    )?,
                ),
                Instruction::BinaryInt {
                    dst,
                    op: operator,
                    lhs: left,
                    rhs: right,
                } => (
                    dst,
                    crate::ops::binary(
                        operator,
                        &slot(&slots, left),
                        &Value::Int(right.into()),
                        &mut diagnostics,
                    )?,
                ),
                Instruction::Unary {
                    dst,
                    op: operator,
                    src,
                } => (
                    dst,
                    crate::ops::unary(operator, &slot(&slots, src), &mut diagnostics)?,
                ),
                Instruction::NewArray { dst, .. } => (dst, Value::Array(Rc::new(PhpArray::new()))),
                Instruction::ArrayPush { array, src: value }
                | Instruction::ArrayPushReference { array, src: value } => {
                    let value = slot(&slots, value);
                    let Some(Some(Value::Array(array))) = slots.get_mut(array.index()) else {
                        return Err(internal_error!("initializer array"));
                    };
                    Rc::make_mut(array).push(value)?;
                    continue;
                }
                Instruction::ArraySet {
                    array,
                    key,
                    src: value,
                }
                | Instruction::ArraySetReference {
                    array,
                    key,
                    src: value,
                } => {
                    let key = ArrayKey::from_value(&slot(&slots, key))?;
                    let value = slot(&slots, value);
                    let Some(Some(Value::Array(array))) = slots.get_mut(array.index()) else {
                        return Err(internal_error!("initializer array"));
                    };
                    Rc::make_mut(array).insert(key, value);
                    continue;
                }
                Instruction::Clear { reg } => {
                    slots[reg.index()] = None;
                    continue;
                }
                Instruction::Return { src } => return Ok(slot(&slots, src)),
                Instruction::ReturnNull => return Ok(Value::Null),
                _ => {
                    return Err(NativeError::error(
                        "Unsupported constant initializer operation",
                    ));
                }
            };
            slots[destination.index()] = Some(value);
        }
        Err(internal_error!("initializer return"))
    }
}

/// Pure initializer temporaries always contain values before their first use.
fn slot(slots: &[Option<Value>], register: Reg) -> Value {
    slots
        .get(register.index())
        .and_then(Option::as_ref)
        .cloned()
        .unwrap_or_default()
}
