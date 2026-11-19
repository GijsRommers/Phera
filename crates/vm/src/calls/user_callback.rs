//! A direct `call_user_func($callback, work())` resolves the callback before
//! evaluating `work()`. The callback enters from the original PHP caller.

use phera_bytecode::{CallbackArguments, Reg};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;

impl Vm {
    pub(crate) async fn autoload_user_callback(
        &mut self,
        callback: Reg,
        arguments: CallbackArguments,
    ) -> Result<(), NativeError> {
        let callback = self.read_local(callback);
        self.autoload_callable(&callback).await?;
        self.prepare_user_callback(callback, arguments)
    }

    pub(crate) fn prepare_user_callback(
        &mut self,
        callback: Value,
        arguments: CallbackArguments,
    ) -> Result<(), NativeError> {
        let callee = self.resolve_callable(&callback).map_err(|error| {
            self.invalid_callback(arguments.function(), 1, "callback", &callback, error, false)
        })?;
        self.prepare_call(callee)
    }

    pub(crate) fn send_user_callback(
        &mut self,
        source: Reg,
        arguments: CallbackArguments,
    ) -> Result<(), NativeError> {
        let value = self.read_local(source);
        match arguments {
            CallbackArguments::Positional => {
                let index = self.call_stack.top_call()?.sent as usize;
                self.store_sent_value(index, value)?;
                self.warn_callback_reference_at(index);
            }
            CallbackArguments::Array => {
                let Value::Array(array) = value else {
                    return Err(Self::callback_array_type_error(value));
                };
                self.route_callback_array(&array)?;
                self.warn_callback_references();
            }
        }
        Ok(())
    }

    pub(crate) fn callback_array_type_error(value: Value) -> NativeError {
        let given = match value {
            Value::Bool(true) => "true".into(),
            Value::Bool(false) => "false".into(),
            value => value.type_name(),
        };
        NativeError::type_error(format!(
            "call_user_func_array(): Argument #2 ($args) must be of type array, {given} given",
        ))
    }

    /// A regular call_user_func can receive names for the callback's parameters.
    pub(crate) async fn invoke_named_user_callback(
        &mut self,
        arguments: Vec<Value>,
        named: &[(usize, phera_value::PhpString)],
    ) -> phera_extension_api::NativeResult {
        use super::callback_binding::ArgumentOrigin;
        use phera_value::{ArrayKey, PhpArray};

        let mut arguments = arguments.into_iter();
        let callback = arguments
            .next()
            .ok_or_else(|| internal_error!("callback argument"))?;
        self.autoload_callable(&callback).await?;
        let callee = self.resolve_callable(&callback).map_err(|error| {
            self.invalid_callback("call_user_func", 1, "callback", &callback, error, false)
        })?;
        let mut forwarded = PhpArray::new();
        let mut names = named.iter().peekable();
        for (offset, value) in arguments.enumerate() {
            if let Some((position, name)) = names.peek()
                && *position == offset + 1
            {
                forwarded.insert(ArrayKey::String(name.clone()), value);
                names.next();
            } else {
                forwarded.push(value)?;
            }
        }
        self.invoke_array_callback_origin(callee, &forwarded, ArgumentOrigin::Callback)
            .await
    }
}
