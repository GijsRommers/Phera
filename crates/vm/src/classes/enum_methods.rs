//! `Suit::from('H')` and `Suit::tryFrom('H')` fetch the same case that `Suit::cases()` lists.

use std::rc::Rc;

use phera_bytecode::EnumBacking;
use phera_extension_api::{Context, NativeError, NativeResult, Param, ParamType};
use phera_value::{ArrayKey, PhpArray, Value};

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
                let stored = class
                    .constants
                    .get(case)
                    .and_then(|constant| self.ready_constant_value(&constant.initializer));
                let object = match stored {
                    Some(object) => object,
                    None => {
                        self.run_initializer(InitializerTask::Case(case_lookup(class, case)?))?
                    }
                };
                result.push(object)?;
            }
            return Ok(Value::Array(Rc::new(result)));
        }
        let Some(backing) = info.backing else {
            return Err(internal_error!("enum lookup without backing"));
        };
        // Fast path for a validated backed enum called with a single exact
        // backing value (e.g. `Status::from('active')` or `$enum::tryFrom($id)`),
        // skipping generic native parameter binding and trace allocation.
        if let [argument] = arguments
            && !self.needs_enum_backing(class)
        {
            let exact_key = match (backing, &*argument.deref()) {
                (EnumBacking::Int, Value::Int(number)) => Some(ArrayKey::Int(*number)),
                (EnumBacking::String, Value::String(string)) => {
                    Some(ArrayKey::String(string.clone()))
                }
                _ => None,
            };
            if let Some(key) = exact_key {
                if let Some(value) = self.backed_enum_case(class, &key) {
                    return Ok(value);
                }
                if matches!(operation, EnumMethod::TryFrom) {
                    return Ok(Value::Null);
                }
            }
        }
        let strict = Context::strict_types(self);
        // PHP's weak null deprecation names the public union even for int
        // backing. The native argument parser converts it to integer zero.
        let weak_null = !strict && matches!(arguments, [argument] if argument.deref().is_null());
        let ty = match (backing, strict, weak_null) {
            (EnumBacking::Int, _, false) => ParamType::Int,
            (EnumBacking::String, true, _) => ParamType::String,
            _ => ParamType::IntOrString,
        };
        self.prepare_call_with_trace(name, &[Param::new("value", ty)], arguments)?;
        let argument = arguments
            .first()
            .ok_or_else(|| internal_error!("enum backing argument"))?;
        let key = match (backing, argument) {
            (EnumBacking::Int, Value::Int(number)) => ArrayKey::Int(*number),
            (EnumBacking::String, Value::Int(number)) => ArrayKey::str(number.to_string()),
            (EnumBacking::String, Value::InternedStr(string)) => ArrayKey::Interned(string),
            (EnumBacking::String, Value::String(string)) => ArrayKey::String(string.clone()),
            _ => return Err(internal_error!("enum backing conversion")),
        };
        if self.needs_enum_backing(class) {
            self.run_initializer(InitializerTask::ValidateEnum(class.clone()))?;
        }
        if let Some(value) = self.backed_enum_case(class, &key) {
            return Ok(value);
        }
        if matches!(operation, EnumMethod::TryFrom) {
            return Ok(Value::Null);
        }
        let value = match key {
            ArrayKey::Int(number) => number.to_string(),
            ArrayKey::Interned(_) | ArrayKey::String(_) => {
                let bytes = key
                    .as_str_bytes()
                    .expect("checked string key")
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
