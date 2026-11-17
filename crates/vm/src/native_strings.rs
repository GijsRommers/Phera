//! A weak native string parameter can run PHP code, e.g. `getenv(new Key())`.
//! Only object arguments need asynchronous preparation for `__toString()`.

use phera_extension_api::{Context, FunctionEntry, NativeError, ParamType};
use phera_value::{PhpArray, Value};

use crate::Vm;
use crate::classes::MethodBody;

impl Vm {
    pub(crate) async fn prepare_stringable_arguments(
        &mut self,
        entry: FunctionEntry,
        arguments: &mut [Value],
    ) -> Result<(), NativeError> {
        let params = entry.params;
        let required = params.iter().filter(|p| !p.optional).count();
        if arguments.len() < required
            || (arguments.len() > params.len() && !params.last().is_some_and(|p| p.variadic))
            || self.strict_types()
        {
            return self.prepare_call_with_trace(entry.name, params, arguments);
        }
        for index in 0..arguments.len() {
            let end = params.len().min(index + 1);
            let param = &params[end - 1];
            let argument = arguments[index].deref().clone();
            if param.ty == ParamType::String
                && !param.by_ref
                && let Value::Object(object) = argument
                && let Some(class) = self.classes.for_object(&object)
                && let Some(method) = class.methods.get("__tostring")
            {
                // Validate earlier arguments before invoking user code with side effects.
                self.prepare_call_with_trace(
                    entry.name,
                    &params[..index.min(params.len())],
                    &mut arguments[..index],
                )?;
                let mut callable = PhpArray::new();
                callable.push(Value::Object(object))?;
                callable.push(Value::from("__toString"))?;
                let result = self.call(&Value::from(callable), Vec::new()).await?;
                let result = result.deref();
                arguments[index] = match &*result {
                    Value::String(_) => result.clone(),
                    Value::Bool(_) | Value::Int(_) | Value::Float(_)
                        if matches!(&method.body, MethodBody::User(f)
                            if !f.unit.functions[f.index].strict_types) =>
                    {
                        Value::String(phera_value::to_php_string(&result, self)?)
                    }
                    other => {
                        return Err(NativeError::type_error(format!(
                            "{}::__toString(): Return value must be of type string, {} returned",
                            method.declaring,
                            other.type_name()
                        )));
                    }
                };
            }
        }
        self.prepare_call_with_trace(entry.name, params, arguments)
    }
}
