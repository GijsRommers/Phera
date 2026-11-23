//! Callable introspection also checks syntax-only shapes: `is_callable('missing', true, $name)`.

use phera_extension_api::NativeResult;
use phera_value::{ArrayKey, Value};

use crate::Vm;

impl Vm {
    /// The output name is written even when resolution fails, as PHP's third argument requires.
    pub(crate) async fn callable_introspection(
        &mut self,
        mut arguments: Vec<Value>,
        origin: crate::calls::callback_binding::ArgumentOrigin,
    ) -> NativeResult {
        const PARAMETERS: &[phera_extension_api::Param] = &[
            phera_extension_api::Param::new("value", phera_extension_api::ParamType::Mixed),
            phera_extension_api::Param::new("syntax_only", phera_extension_api::ParamType::Bool)
                .optional(),
            phera_extension_api::Param::new("callable_name", phera_extension_api::ParamType::Mixed)
                .optional()
                .by_ref(),
        ];
        self.with_argument_origin(origin, |vm| {
            phera_extension_api::prepare_call("is_callable", PARAMETERS, &mut arguments, vm)
        })?;
        let value = arguments.first().cloned().unwrap_or_default();
        let syntax_only = arguments.get(1).is_some_and(phera_value::to_bool);
        let shape = callable_shape(&value);
        let valid = if syntax_only && !value.is_object() {
            shape.is_some()
        } else {
            self.autoload_callable(&value).await?;
            self.resolve_callable(&value).is_ok()
        };
        if let Some(Value::Reference(reference)) = arguments.get(2) {
            *reference.borrow_mut() = match shape {
                Some(shape) => Value::from(shape),
                None if matches!(value, Value::Array(_)) => Value::from("Array"),
                None => Value::from(phera_value::to_php_string(
                    &value,
                    &mut phera_value::NoDiagnostics,
                )?),
            };
        }
        Ok(Value::Bool(valid))
    }
}

/// Callable names describe objects by class rather than running conversion hooks.
fn callable_shape(value: &Value) -> Option<String> {
    match &*value.deref() {
        Value::InternedStr(name) => Some(name.to_string_lossy()),
        Value::String(name) => Some(name.to_string_lossy()),
        Value::Object(object) if object.class.name() == "Closure" => {
            let native = object.native.borrow();
            native
                .as_ref()
                .and_then(|state| state.downcast_ref::<crate::closures::ClosureState>())
                .map(|state| state.callee.name.to_string())
        }
        Value::Object(object) => Some(format!("{}::__invoke", object.class.name())),
        Value::Array(array) if array.len() == 2 => {
            let target = array.get(&ArrayKey::Int(0))?;
            let method = array.get(&ArrayKey::Int(1))?;
            let method = method.deref().str_ref()?.to_string_lossy();
            let class = match &*target.deref() {
                Value::InternedStr(name) => name.to_string_lossy(),
                Value::String(name) => name.to_string_lossy(),
                Value::Object(object) => object.class.name().to_owned(),
                _ => return None,
            };
            Some(format!("{class}::{method}"))
        }
        _ => None,
    }
}
