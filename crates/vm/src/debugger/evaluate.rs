//! `eval -i 9 -- JGNvdW50ICsgMQ==` evaluates `$count + 1` in the stopped frame.

use std::sync::Arc;

use phera_extension_api::NativeResult;

use crate::Vm;

impl Vm {
    pub(super) async fn debug_condition(&mut self, expression: &[u8]) -> bool {
        self.debug_eval(expression)
            .await
            .is_ok_and(|value| phera_value::to_bool(&value))
    }

    pub(super) async fn debug_eval(&mut self, expression: &[u8]) -> NativeResult {
        if self
            .state
            .get::<phera_extension_api::ImageSeal>()
            .is_some_and(|seal| seal.forbid_eval)
        {
            return Err(phera_extension_api::NativeError::error(
                "eval() is disabled in strict AOT image mode",
            ));
        }
        let mut script = b"return (".to_vec();
        script.extend_from_slice(expression);
        script.extend_from_slice(b"\n);");
        let seed = u64::from(self.code.next_unit_index().0) << 32;
        let file = format!("dbgp://{}", self.code.next_unit_index().0);
        let unit = tokio::task::spawn_blocking(move || {
            phera_compiler::compile_eval_with_seed(&script, &file, seed)
        })
        .await
        .map_err(|error| phera_extension_api::NativeError::error(error.to_string()))?
        .map_err(|error| phera_extension_api::NativeError::error(error.to_string()))?;
        let depth = self.call_stack.depth();
        let previous = std::mem::replace(&mut self.debug_evaluating, true);
        let result = match self.enter_scoped_unit(Arc::new(unit), None) {
            Ok(()) => self.interpret(depth).await,
            Err(error) => Err(error),
        };
        self.debug_evaluating = previous;
        result
    }
}
