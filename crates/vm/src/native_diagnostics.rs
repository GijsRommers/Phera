//! Native warning snapshots follow argument conversion without changing the extension API.

use crate::Vm;
use phera_extension_api::{NativeError, Param, prepare_call};
use phera_value::Value;

impl Vm {
    /// Without a user handler, use the ordinary one-pass native parser. With a
    /// handler, prepared prefixes preserve PHP's argument values at each warning.
    pub(crate) fn prepare_call_with_trace(
        &mut self,
        name: &str,
        parameters: &[Param],
        arguments: &mut [Value],
    ) -> Result<(), NativeError> {
        self.prepare_call_recording(
            name,
            parameters,
            arguments,
            crate::calls::callback_binding::ArgumentBinding::Caller,
        )
    }

    pub(crate) fn prepare_call_recording(
        &mut self,
        name: &str,
        parameters: &[Param],
        arguments: &mut [Value],
        binding: crate::calls::callback_binding::ArgumentBinding,
    ) -> Result<(), NativeError> {
        let callbacks = parameters
            .iter()
            .any(|parameter| parameter.ty == phera_extension_api::ParamType::Callable);
        let traced = binding.has_native_frame()
            && self.error_handlers.error.is_some()
            && !self.error_handlers.active;
        if !callbacks && !traced {
            return prepare_call(name, parameters, arguments, self);
        }
        let required = parameters
            .iter()
            .filter(|parameter| !parameter.optional)
            .count();
        let variadic = parameters
            .last()
            .is_some_and(|parameter| parameter.variadic);
        if arguments.len() < required || (!variadic && arguments.len() > parameters.len()) {
            return prepare_call(name, parameters, arguments, self);
        }
        for index in 0..arguments.len() {
            if traced {
                self.error_handlers.native_site =
                    Some(self.native_parameter_trace(name, parameters, arguments));
            }
            let parameter_end = (index + 1).min(parameters.len());
            prepare_call(
                name,
                &parameters[..parameter_end],
                &mut arguments[..=index],
                self,
            )?;
            if let Some(parameter) = parameters
                .get(index)
                .filter(|parameter| parameter.ty == phera_extension_api::ParamType::Callable)
            {
                let value = &arguments[index];
                if parameter.nullable && value.is_null() {
                    continue;
                }
                self.resolve_callable(value).map_err(|error| {
                    self.invalid_callback(
                        name,
                        index + 1,
                        parameter.name,
                        value,
                        error,
                        parameter.nullable,
                    )
                })?;
            }
        }
        Ok(())
    }
}
