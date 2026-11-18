//! `Color::Red === Color::Red` because the request owns one object per case.

use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use phera_bytecode::EnumBacking;
use phera_extension_api::{Context, NativeError, NativeResult, Param, ParamType};
use phera_value::{ArrayKey, Object, PhpArray, Value};

use super::{Class, ClassIndex, class::ConstantInitializer, enums::EnumMethod};
use crate::{Vm, code_table::FunctionIndex};

type BackingTable = HashMap<ArrayKey, Rc<Object>>;

#[derive(Default)]
pub(crate) struct EnumCases {
    objects: HashMap<ClassIndex, HashMap<String, Rc<Object>>>,
    initializing: HashSet<(ClassIndex, String)>,
    backed: HashMap<ClassIndex, BackingTable>,
    validating: HashSet<ClassIndex>,
}

impl Vm {
    pub(crate) fn enum_case(
        &mut self,
        class: &Rc<Class>,
        name: &str,
        initializer: Option<FunctionIndex>,
        spelling: Option<&str>,
    ) -> Result<Rc<Object>, NativeError> {
        if let Some(object) = self
            .enum_cases
            .objects
            .get(&class.index)
            .and_then(|cases| cases.get(name))
        {
            return Ok(object.clone());
        }
        let identity = (class.index, name.to_owned());
        if !self.enum_cases.initializing.insert(identity.clone()) {
            return Err(NativeError::error(format!(
                "Cannot declare self-referencing constant {}::{name}",
                spelling.unwrap_or(&class.name)
            )));
        }
        let backing = initializer
            .map(|initializer| self.evaluate_initializer(&initializer, &class.name))
            .transpose();
        self.enum_cases.initializing.remove(&identity);
        let backing = backing?;
        let object = self.classes.allocate(class.clone());
        object.set(ArrayKey::str("name"), Value::from(name));
        if let Some(value) = backing {
            object.set(ArrayKey::str("value"), value);
        }
        self.enum_cases
            .objects
            .entry(class.index)
            .or_default()
            .insert(identity.1, object.clone());
        Ok(object)
    }

    pub(crate) fn validate_enum_backing(&mut self, class: &Rc<Class>) -> Result<(), NativeError> {
        let Some(info) = &class.enum_info else {
            return Ok(());
        };
        let Some(backing) = info.backing else {
            return Ok(());
        };
        if self.enum_cases.backed.contains_key(&class.index)
            || !self.enum_cases.validating.insert(class.index)
        {
            return Ok(());
        }
        let result = self.build_enum_table(class, backing, &info.names);
        self.enum_cases.validating.remove(&class.index);
        self.enum_cases.backed.insert(class.index, result?);
        Ok(())
    }

    fn build_enum_table(
        &mut self,
        class: &Rc<Class>,
        backing: EnumBacking,
        names: &[String],
    ) -> Result<BackingTable, NativeError> {
        let mut table: BackingTable = HashMap::with_capacity(names.len());
        for name in names {
            let object = self.enum_case_named(class, name)?;
            let value = object
                .get(&ArrayKey::str("value"))
                .ok_or_else(|| internal_error!("backed enum value"))?;
            let key = match (backing, value) {
                (EnumBacking::Int, Value::Int(number)) => ArrayKey::Int(number),
                (EnumBacking::String, Value::String(string)) => ArrayKey::String(string),
                (backing, value) => {
                    let expected = match backing {
                        EnumBacking::Int => "int",
                        EnumBacking::String => "string",
                    };
                    return Err(NativeError::type_error(format!(
                        "Enum case type {} does not match enum backing type {expected}",
                        value.type_name(),
                    )));
                }
            };
            if let Some(previous) = table.insert(key, object) {
                let previous_name = previous.get(&ArrayKey::str("name")).unwrap_or_default();
                let Value::String(previous_name) = previous_name else {
                    return Err(internal_error!("enum case name"));
                };
                return Err(NativeError::error(format!(
                    "Duplicate value in enum {} for cases {} and {name}",
                    class.name,
                    previous_name.to_string_lossy(),
                )));
            }
        }
        Ok(table)
    }

    fn enum_case_named(
        &mut self,
        class: &Rc<Class>,
        name: &str,
    ) -> Result<Rc<Object>, NativeError> {
        let Some(constant) = class.constants.get(name) else {
            return Err(internal_error!("enum case constant"));
        };
        let ConstantInitializer::EnumCase { backing, .. } = constant.initializer else {
            return Err(internal_error!("enum case constant initializer"));
        };
        self.enum_case(class, name, backing, None)
    }

    pub(crate) fn call_enum_method(
        &mut self,
        operation: EnumMethod,
        name: &str,
        class: &Rc<Class>,
        arguments: &mut [Value],
    ) -> NativeResult {
        let Some(info) = &class.enum_info else {
            return Err(internal_error!("enum method scope"));
        };
        if matches!(operation, EnumMethod::Cases) {
            self.prepare_call_with_trace(name, &[], arguments)?;
            let mut result = PhpArray::new();
            for case in &info.names {
                result.push(Value::Object(self.enum_case_named(class, case)?))?;
            }
            return Ok(Value::Array(Rc::new(result)));
        }
        let Some(backing) = info.backing else {
            return Err(internal_error!("enum lookup without backing"));
        };
        let strict = Context::strict_types(self);
        let ty = match backing {
            EnumBacking::Int => ParamType::Int,
            EnumBacking::String if strict => ParamType::String,
            EnumBacking::String => ParamType::IntOrString,
        };
        if !strict && let [argument] = &*arguments {
            let argument = argument.deref();
            if argument.is_null() {
                return Err(NativeError::throw(
                    "Phera\\NotSupportedException",
                    format!(
                        "Passing null to {name}() is not supported by Phera: deprecated since PHP 8.1",
                    ),
                ));
            }
        }
        self.prepare_call_with_trace(name, &[Param::new("value", ty)], arguments)?;
        let argument = arguments
            .first()
            .ok_or_else(|| internal_error!("enum backing argument"))?;
        let key = match (backing, argument) {
            (EnumBacking::Int, Value::Int(number)) => ArrayKey::Int(*number),
            (EnumBacking::String, Value::Int(number)) => ArrayKey::str(number.to_string()),
            (EnumBacking::String, Value::String(string)) => ArrayKey::String(string.clone()),
            _ => return Err(internal_error!("enum backing conversion")),
        };
        self.validate_enum_backing(class)?;
        if let Some(object) = self
            .enum_cases
            .backed
            .get(&class.index)
            .and_then(|table| table.get(&key))
        {
            return Ok(Value::Object(object.clone()));
        }
        if matches!(operation, EnumMethod::TryFrom) {
            return Ok(Value::Null);
        }
        let value = match key {
            ArrayKey::Int(number) => number.to_string(),
            ArrayKey::String(string) => {
                let bytes = string
                    .as_bytes()
                    .split(|byte| *byte == 0)
                    .next()
                    .unwrap_or_default();
                format!("\"{}\"", String::from_utf8_lossy(bytes))
            }
        };
        Err(NativeError::value_error(format!(
            "{value} is not a valid backing value for enum {}",
            class.name
        )))
    }
}
