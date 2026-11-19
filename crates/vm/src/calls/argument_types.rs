//! PHP parameter coercion after argument evaluation.

use crate::{Vm, types};
use phera_extension_api::{Context, NativeError, NativeResult};
use phera_value::Value;

impl Vm {
    /// Coercion of a reference updates its cell so the caller observes it.
    #[inline]
    pub(crate) fn check_argument(
        &mut self,
        value: Value,
        declaration_type: &phera_bytecode::Type,
        function: &str,
        index: usize,
        parameter: (&str, Option<&crate::classes::MethodContext>),
    ) -> NativeResult {
        let crate::diagnostics::SourceLocation { file, line } = self.location();
        let location_suffix = self.argument_origin.location_suffix(&file, line);
        let (parameter, scope) = parameter;
        let result = types::coerce_in_scope(
            value.deref().clone(),
            declaration_type,
            self.strict_types().into(),
            self,
            scope,
        )
        .map_err(|error| match error {
            NativeError::Throw { class, message, .. } => {
                let number = index + 1;
                let label = if parameter.is_empty() {
                    String::new()
                } else {
                    format!(" (${parameter})")
                };
                let message =
                    format!("{function}(): Argument #{number}{label} {message}{location_suffix}");
                NativeError::throw(class, message)
            }
            other => other,
        })?;
        if let Value::Reference(reference) = value {
            *reference
                .try_borrow_mut()
                .map_err(|_| internal_error!("argument reference"))? = result;
            Ok(Value::Reference(reference))
        } else {
            Ok(result)
        }
    }
}
