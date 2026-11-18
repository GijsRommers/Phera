//! `ob_start(function ($text) { echo 'extra'; return $text; });`
//! emits a deprecation when the handler runs.
//!
//! PHP 8.5 lets a user handler finish after reporting its attempt to print.

use phera_extension_api::NativeError;

use crate::{Vm, error_traces::NativeTrace};

use super::Operation;

impl Vm {
    /// Temporarily route the diagnostic through outer buffers. PHP still uses
    /// the callback's return value if a user error handler throws.
    pub(super) async fn report_handler_output(
        &mut self,
        index: usize,
        operation: Operation,
    ) -> Result<(), NativeError> {
        let caller = operation.caller();
        let prefix = if self.output_writer.buffers.finalizing {
            "PHP Request Shutdown".to_owned()
        } else if let Some(function) = caller {
            format!("{function}()")
        } else {
            match self.current() {
                Some(frame) if frame.function_id().is_some() => {
                    format!("{}()", frame.function().name)
                }
                Some(_) => "main()".to_owned(),
                None => "Unknown".to_owned(),
            }
        };
        let buffer = self
            .output_writer
            .buffers
            .stack
            .get_mut(index)
            .ok_or_else(|| internal_error!("output handler deprecation buffer"))?;
        buffer.disabled = true;
        let message = format!(
            "{prefix}: Producing output from user output handler {} is deprecated",
            buffer.name,
        );
        if self.handles_diagnostic(8192) {
            let previous = self.error_handlers.native_site.take();
            self.error_handlers.native_site = caller
                .map(|function| NativeTrace::new(function, &[]))
                .or_else(|| previous.clone());
            self.diagnostic("Deprecated", message);
            self.error_handlers.native_site = previous;
        } else {
            self.diagnostic("Deprecated", message);
        }
        let result = self.drain_error_handlers().await;
        if let Some(buffer) = self.output_writer.buffers.stack.get_mut(index) {
            buffer.disabled = false;
        }
        result
    }
}
