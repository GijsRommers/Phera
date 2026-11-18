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

pub(crate) struct InitializerFrame {
    reference: FunctionIndex,
    pub(crate) scope: String,
    slots: Vec<Option<Value>>,
    position: usize,
    prepared: Option<crate::calls::call_state::Callee>,
    diagnostics: phera_value::CollectDiagnostics,
    relative_names: std::collections::HashMap<Reg, phera_bytecode::SymbolId>,
}

pub(crate) enum InitializerStep {
    Done(Value),
    Read(crate::initializer_tasks::ConstantLookup, Reg),
}

impl InitializerFrame {
    pub(crate) fn new(
        code: &crate::code_table::CodeTable,
        reference: FunctionIndex,
        scope: String,
    ) -> Self {
        let registers = code.function(reference).registers;
        Self {
            reference,
            scope,
            slots: vec![None; usize::from(registers)],
            position: 0,
            prepared: None,
            diagnostics: phera_value::CollectDiagnostics::default(),
            relative_names: std::collections::HashMap::new(),
        }
    }

    pub(crate) fn store(&mut self, destination: Reg, value: Value) {
        if let Some(slot) = self.slots.get_mut(destination.index()) {
            *slot = Some(value);
        }
    }
}

impl Vm {
    /// Initializers have private registers and cannot mutate the caller's locals.
    pub(crate) fn evaluate_initializer(
        &mut self,
        reference: &FunctionIndex,
        scope: &str,
    ) -> NativeResult {
        self.run_initializer(crate::initializer_tasks::InitializerTask::Expression(
            *reference,
            scope.to_owned(),
        ))
    }

    /// Evaluate until the expression returns or needs another class constant.
    pub(crate) fn advance_initializer(
        &mut self,
        frame: &mut InitializerFrame,
    ) -> Result<InitializerStep, NativeError> {
        let InitializerFrame {
            reference,
            scope,
            slots,
            position,
            prepared,
            diagnostics,
            relative_names,
        } = frame;
        let unit = self.code.unit(*reference).clone();
        let function = crate::code_table::body(&unit, self.code.local(*reference));
        while let Some(instruction) = function.instructions.get(*position).copied() {
            *position += 1;
            if crate::fast::scalar_step(instruction, slots, position)
                == crate::fast::ScalarStep::Complete
            {
                continue;
            }
            match self.initializer_callable(instruction, reference, scope, prepared, slots)? {
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
                    let symbol = class;
                    let name = &unit.symbols[symbol.index()].written;
                    relative_names.remove(&dst);
                    if matches!(name.as_str(), "self" | "parent" | "static") {
                        relative_names.insert(dst, symbol);
                    }
                    let name = match name.as_str() {
                        "self" | "static" | phera_bytecode::TRAIT_CLASS_NAME => scope.clone(),
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
                    let name = &unit.names[name.index()];
                    if name.eq_ignore_ascii_case("class")
                        && let Value::Object(object) = slot(slots, class)
                    {
                        (dst, Value::from(object.class.name()))
                    } else {
                        let lookup = make_lookup(
                            &unit,
                            relative_names,
                            class,
                            slot(slots, class),
                            name.clone(),
                        )?;
                        return Ok(InitializerStep::Read(lookup, dst));
                    }
                }
                Instruction::FetchDynamicClassConstant { dst, class, name } => {
                    let name = crate::class_constants::constant_name(slot(slots, name))?;
                    let lookup =
                        make_lookup(&unit, relative_names, class, slot(slots, class), name)?;
                    return Ok(InitializerStep::Read(lookup, dst));
                }
                Instruction::FetchConstant { dst, constant } => {
                    (dst, self.constants.fetch(unit.clone(), constant)?)
                }
                Instruction::Fetch { dst, path, .. } => (
                    dst,
                    self.initializer_path(&unit, &function.paths[path.index()], slots)?,
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
                        &slot(slots, left),
                        &slot(slots, right),
                        diagnostics,
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
                        &slot(slots, left),
                        &Value::Int(right.into()),
                        diagnostics,
                    )?,
                ),
                Instruction::Unary {
                    dst,
                    op: operator,
                    src,
                } => (
                    dst,
                    crate::ops::unary(operator, &slot(slots, src), diagnostics)?,
                ),
                Instruction::NewArray { dst, .. } => (dst, Value::Array(Rc::new(PhpArray::new()))),
                Instruction::ArrayPush { array, src: value }
                | Instruction::ArrayPushReference { array, src: value } => {
                    let value = slot(slots, value);
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
                    let key = ArrayKey::from_value(&slot(slots, key))?;
                    let value = slot(slots, value);
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
                Instruction::Return { src } => return Ok(InitializerStep::Done(slot(slots, src))),
                Instruction::ReturnNull => return Ok(InitializerStep::Done(Value::Null)),
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

fn make_lookup(
    unit: &phera_bytecode::CompiledUnit,
    relative_names: &mut std::collections::HashMap<Reg, phera_bytecode::SymbolId>,
    register: Reg,
    value: Value,
    name: String,
) -> Result<crate::initializer_tasks::ConstantLookup, NativeError> {
    let class = match value {
        Value::String(name) => name.to_string_lossy(),
        Value::Object(object) => object.class.name().to_owned(),
        _ => {
            return Err(NativeError::error(
                "Class name must be a valid object or a string",
            ));
        }
    };
    let spelling = relative_names
        .remove(&register)
        .map(|symbol| unit.symbols[symbol.index()].written.clone());
    Ok(crate::initializer_tasks::ConstantLookup {
        class,
        name,
        spelling,
    })
}

/// Pure initializer temporaries always contain values before their first use.
fn slot(slots: &[Option<Value>], register: Reg) -> Value {
    slots
        .get(register.index())
        .and_then(Option::as_ref)
        .cloned()
        .unwrap_or_default()
}
