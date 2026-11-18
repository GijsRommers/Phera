//! Pure default-value functions reuse scalar bytecode semantics without invoking PHP callbacks.

use crate::{Vm, code_table::FunctionIndex};
use phera_bytecode::{Instruction, Reg};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, PhpArray, Value};
use std::rc::Rc;

/// Trait property defaults may consult constants from the class still being linked.
pub(crate) struct InitializerClass {
    pub(crate) name: String,
    pub(crate) constants: std::collections::HashMap<String, Rc<crate::classes::LinkedConstant>>,
}

impl Vm {
    /// Initializers have private registers and cannot mutate the caller's locals.
    pub(crate) fn evaluate_initializer(
        &mut self,
        reference: &FunctionIndex,
        scope: &str,
    ) -> NativeResult {
        if self.initializer_depth == 128 {
            return Err(NativeError::error(
                "Constant expression nesting exceeds Phera's limit",
            ));
        }
        self.initializer_depth += 1;
        let previous = self.initializer_scope.replace(scope.to_owned());
        let result = self.evaluate_initializer_body(reference, scope);
        self.initializer_scope = previous;
        self.initializer_depth -= 1;
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
        let mut relative_names = std::collections::HashMap::new();
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
                    relative_names.remove(&dst);
                    if matches!(name.as_str(), "self" | "parent" | "static") {
                        relative_names.insert(dst, name.as_str());
                    }
                    let name = match name.as_str() {
                        "self" | "static" | phera_bytecode::TRAIT_CLASS_NAME => scope.to_owned(),
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
                    let spelling = relative_names.remove(&class);
                    let class = slot(&slots, class);
                    let Value::String(class) = class else {
                        return Err(internal_error!("initializer class"));
                    };
                    (
                        dst,
                        self.class_constant_written(
                            &class.to_string_lossy(),
                            &unit.names[name.index()],
                            spelling,
                        )?,
                    )
                }
                Instruction::FetchDynamicClassConstant { dst, class, name } => {
                    let Value::String(class) = slot(&slots, class) else {
                        return Err(internal_error!("initializer class"));
                    };
                    (
                        dst,
                        self.dynamic_class_constant(&class.to_string_lossy(), &slot(&slots, name))?,
                    )
                }
                Instruction::FetchConstant { dst, constant } => {
                    (dst, self.constants.fetch(unit.clone(), constant)?)
                }
                Instruction::Fetch { dst, path, .. } => (
                    dst,
                    self.initializer_path(&unit, &function.paths[path.index()], &slots)?,
                ),
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

    fn initializer_path(
        &mut self,
        unit: &phera_bytecode::CompiledUnit,
        path: &phera_bytecode::Path,
        slots: &[Option<Value>],
    ) -> NativeResult {
        use phera_bytecode::{PathBase, Step};
        let PathBase::Register(root) = path.base else {
            return Err(internal_error!("constant expression path root"));
        };
        let mut value = slot(slots, root);
        for step in &path.steps {
            value = match *step {
                Step::Property(name) => {
                    self.initializer_property(value, &unit.names[name.index()])?
                }
                Step::DynamicProperty(name) => {
                    let name = phera_value::to_php_string(
                        &slot(slots, name),
                        &mut phera_value::NoDiagnostics,
                    )?;
                    self.initializer_property(value, &name.to_string_lossy())?
                }
                Step::Index(index) => {
                    let Value::Array(array) = value else {
                        return Err(NativeError::error(
                            "Cannot use [] on objects in constant expression",
                        ));
                    };
                    let key = ArrayKey::from_value(&slot(slots, index))?;
                    array.get(&key).cloned().unwrap_or_default()
                }
                Step::Append => return Err(internal_error!("constant expression append")),
            };
        }
        Ok(value)
    }

    fn initializer_property(&mut self, value: Value, name: &str) -> NativeResult {
        if let Value::Object(object) = value {
            if !object.class.is_enum() {
                return Err(NativeError::error(
                    "Fetching properties on non-enums in constant expressions is not allowed",
                ));
            }
            if let Some(value) = object.get(&ArrayKey::str(name)) {
                return Ok(value);
            }
            self.diagnostic(
                "Warning",
                format!("Undefined property: {}::${name}", object.class.name()),
            );
        } else {
            self.diagnostic(
                "Warning",
                format!(
                    "Attempt to read property \"{name}\" on {}",
                    value.type_name()
                ),
            );
        }
        Ok(Value::Null)
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
