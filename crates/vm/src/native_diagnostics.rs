//! Native warning snapshots follow argument conversion without changing the extension API.

use crate::{Vm, error_traces::NativeTrace};
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
        if self.error_handlers.error.is_none() || self.error_handlers.active {
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
            self.error_handlers.native_site = Some(NativeTrace::new(name, arguments));
            let parameter_end = (index + 1).min(parameters.len());
            prepare_call(
                name,
                &parameters[..parameter_end],
                &mut arguments[..=index],
                self,
            )?;
        }
        Ok(())
    }
}
