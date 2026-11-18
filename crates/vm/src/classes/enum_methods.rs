//! `Suit::from('H')` and `Suit::tryFrom('H')` fetch the same case that `Suit::cases()` lists.

use std::rc::Rc;

use phera_bytecode::EnumBacking;
use phera_extension_api::{Context, NativeError, NativeResult, Param, ParamType};
use phera_value::{ArrayKey, NumVal, PhpArray, Value};

use super::{Class, enum_cases::case_lookup, enums::EnumMethod};
use crate::{Vm, initializer_tasks::InitializerTask};

impl Vm {
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
                result.push(
                    self.run_initializer(InitializerTask::Case(case_lookup(class, case)?))?,
                )?;
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
            reject_deprecated_lookup_argument(name, ty, &argument.deref())?;
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
        self.run_initializer(InitializerTask::ValidateEnum(class.clone()))?;
        if let Some(value) = self.backed_enum_case(class, &key) {
            return Ok(value);
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

/// Legacy null and lossy float coercion produce a clear error in Phera.
fn reject_deprecated_lookup_argument(
    name: &str,
    ty: ParamType,
    argument: &Value,
) -> Result<(), NativeError> {
    if argument.is_null() {
        return Err(crate::function_introspection::deprecated(
            &format!("Passing null to {name}()"),
            "8.1",
        ));
    }
    let float = match argument {
        Value::Float(value) => Some(*value),
        Value::String(value) if ty == ParamType::Int => {
            let parsed = phera_value::parse_numeric(value.as_bytes());
            match parsed.value {
                Some(NumVal::Float(value)) if parsed.exact => Some(value),
                _ => None,
            }
        }
        _ => None,
    };
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    if float.is_some_and(|float| (-LIMIT..LIMIT).contains(&float) && float.fract() != 0.0) {
        return Err(crate::function_introspection::deprecated(
            &format!("Implicit conversion to int in {name}() that loses precision"),
            "8.1",
        ));
    }
    Ok(())
}
