//! `ob_flush()` sends a transformed chunk to the next outer buffer. An
//! output handler returning `false` passes the original bytes through and
//! is disabled for subsequent writes.

use phera_extension_api::{Context, NativeError, NativeResult};
use phera_value::{Value, to_php_string};

use crate::Vm;

use super::{CLEANABLE, FLUSHABLE, OutputFunction, REMOVABLE, Running};

#[derive(Clone, Copy)]
pub(crate) enum Operation {
    Chunk(Option<&'static str>),
    Clean,
    Flush,
    EndClean,
    EndFlush,
    GetClean,
    GetFlush,
    Shutdown,
}

impl Operation {
    fn mode(self) -> i64 {
        match self {
            Self::Chunk(_) => 0,
            Self::Clean => 2,
            Self::Flush => 4,
            Self::EndClean | Self::GetClean => 8 | 2,
            Self::EndFlush | Self::GetFlush | Self::Shutdown => 8,
        }
    }

    fn ability(self) -> i64 {
        match self {
            Self::Clean => CLEANABLE,
            Self::Flush => FLUSHABLE,
            _ => REMOVABLE,
        }
    }

    pub(super) fn caller(self) -> Option<&'static str> {
        Some(match self {
            Self::Clean => OutputFunction::Clean.name(),
            Self::Flush => OutputFunction::FlushBuffer.name(),
            Self::EndClean => OutputFunction::EndClean.name(),
            Self::EndFlush => OutputFunction::EndFlush.name(),
            Self::GetClean => OutputFunction::GetClean.name(),
            Self::GetFlush => OutputFunction::GetFlush.name(),
            Self::Chunk(caller) => return caller,
            Self::Shutdown => return None,
        })
    }

    fn removes(self) -> bool {
        matches!(
            self,
            Self::EndClean | Self::EndFlush | Self::GetClean | Self::GetFlush | Self::Shutdown
        )
    }

    fn discards(self) -> bool {
        matches!(self, Self::Clean | Self::EndClean | Self::GetClean)
    }

    fn reads(self) -> bool {
        matches!(self, Self::GetClean | Self::GetFlush)
    }

    fn empty_notice(self) -> Option<&'static str> {
        match self {
            Self::Clean | Self::EndClean => Some("Failed to delete buffer. No buffer to delete"),
            Self::Flush => Some("Failed to flush buffer. No buffer to flush"),
            Self::GetFlush | Self::EndFlush => {
                Some("Failed to delete and flush buffer. No buffer to delete or flush")
            }
            _ => None,
        }
    }

    fn denied(self) -> &'static str {
        match self {
            Self::Flush => "flush",
            Self::EndFlush | Self::GetFlush => "send",
            Self::EndClean | Self::GetClean => "discard",
            _ => "delete",
        }
    }
}

impl Vm {
    pub(crate) fn check_output_handler(
        &mut self,
        function: &str,
        arguments: &[Value],
    ) -> Result<(), NativeError> {
        if self.output_writer.buffers.running.is_none() {
            return Ok(());
        }
        self.output_writer.buffers.lock_broken = true;
        Err(self.fatal_native_error(
            1,
            format!(
                "{function}(): Cannot use output buffering in output buffering display handlers"
            ),
            function,
            arguments,
        ))
    }

    pub(crate) async fn change_buffer(
        &mut self,
        function: OutputFunction,
        operation: Operation,
    ) -> NativeResult {
        self.check_output_handler(function.name(), &[])?;
        let stack = &self.output_writer.buffers.stack;
        let Some((index, buffer)) = stack.iter().enumerate().next_back() else {
            if let Some(message) = operation.empty_notice() {
                self.buffer_notice(function, format!("{}(): {message}", function.name()));
            }
            return Ok(Value::Bool(false));
        };
        let permitted = buffer.settings & operation.ability() != 0;
        let captured = operation
            .reads()
            .then(|| Value::from(buffer.bytes.as_slice()));
        if !permitted {
            let action = operation.denied();
            let name = buffer.name.clone();
            self.buffer_notice(
                function,
                format!(
                    "{}(): Failed to {action} buffer of {name} ({index})",
                    function.name()
                ),
            );
            if matches!(operation, Operation::GetClean | Operation::GetFlush) {
                self.buffer_notice(
                    function,
                    format!(
                        "{}(): Failed to delete buffer of {name} ({index})",
                        function.name()
                    ),
                );
            }
            return Ok(captured.unwrap_or(Value::Bool(false)));
        }
        self.apply_buffer(index, operation).await?;
        self.drain_output_from(Some(function.name())).await?;
        Ok(captured.unwrap_or(Value::Bool(true)))
    }

    fn buffer_notice(&mut self, function: OutputFunction, message: String) {
        if self.handles_diagnostic(8) {
            let native = crate::error_traces::NativeTrace::new(function.name(), &[]);
            let previous = self.error_handlers.native_site.replace(native);
            self.notice(message);
            self.error_handlers.native_site = previous;
        } else {
            self.notice(message);
        }
    }

    pub(crate) async fn drain_output(&mut self) -> Result<(), NativeError> {
        let caller = self.output_writer.chunk_caller.take();
        self.drain_output_from(caller).await
    }

    pub(crate) async fn drain_output_from(
        &mut self,
        caller: Option<&'static str>,
    ) -> Result<(), NativeError> {
        while let Some(index) = self.output_writer.buffers.pending.take() {
            self.refresh_pending_callbacks();
            self.apply_buffer(index, Operation::Chunk(caller)).await?;
        }
        if self.headers_pending() {
            self.finish_headers().await?;
        }
        if self
            .output_writer
            .response
            .as_ref()
            .is_some_and(|response| response.pending)
        {
            self.flush_response_body().await?;
        }
        Ok(())
    }

    pub(crate) async fn finish_buffers(&mut self) {
        self.output_writer.buffers.finalizing = true;
        while let Some(index) = self.output_writer.buffers.len().checked_sub(1) {
            if let Err(error) = self.apply_buffer(index, Operation::Shutdown).await {
                self.output_writer.buffers.stack.clear();
                self.output_writer.buffers.pending = None;
                match &error {
                    NativeError::Unwind(state) if state.0.is::<crate::exceptions::Thrown>() => {
                        self.status = 255;
                    }
                    _ => self.unwind(error),
                }
                break;
            }
            if let Err(error) = self.drain_output().await {
                self.unwind(error);
                self.output_writer.buffers.stack.clear();
                self.output_writer.buffers.pending = None;
                break;
            }
        }
        self.output_writer.buffers.finalizing = false;
    }

    async fn apply_buffer(
        &mut self,
        index: usize,
        operation: Operation,
    ) -> Result<(), NativeError> {
        let buffer = self
            .output_writer
            .buffers
            .stack
            .get(index)
            .ok_or_else(|| internal_error!("output buffer index"))?;
        let callback = if buffer.disabled {
            None
        } else {
            buffer.callback.clone()
        };
        let mode = operation.mode() | i64::from(!buffer.started);
        let mut deprecation = Ok(());
        let result = if let Some(callback) = callback {
            let arguments = [Value::from(buffer.bytes.as_slice()), Value::Int(mode)];
            let previous = self
                .output_writer
                .buffers
                .running
                .replace(Running { index });
            use crate::error_traces::{NativeCaller, NativeTrace};
            let native = operation
                .caller()
                .map_or(NativeCaller::Internal(None), |name| {
                    NativeCaller::Saved(NativeTrace::new(name, &[]))
                });
            self.push_native_callback(native);
            let result = self.invoke_resolved_callable(callback, arguments).await;
            self.native_callback_sites.pop();
            let buffers = &mut self.output_writer.buffers;
            if buffers.lock_broken {
                if previous.is_none() {
                    buffers.lock_broken = false;
                }
                buffers.running = previous;
                buffers.stack.clear();
                buffers.pending = None;
                return result.map(|_| ());
            }
            let buffer = buffers
                .stack
                .get_mut(index)
                .ok_or_else(|| internal_error!("output buffer index after callback"))?;
            if result.is_ok() && std::mem::take(&mut buffer.produced_output) {
                deprecation = self.report_handler_output(index, operation).await;
            }
            let buffers = &mut self.output_writer.buffers;
            buffers.running = previous;
            if buffers.lock_broken {
                if buffers.running.is_none() {
                    buffers.lock_broken = false;
                }
                return deprecation;
            }
            result
        } else {
            Ok(Value::Bool(false))
        };
        let replacement = match result {
            Ok(Value::Bool(false)) => Ok(None),
            Ok(Value::Bool(true)) => Ok(Some(Vec::new())),
            Ok(value) => to_php_string(&value, self.diagnostics())
                .map(|string| Some(string.as_bytes().to_vec()))
                .map_err(NativeError::from),
            Err(error) => Err(error),
        };
        let buffer = self
            .output_writer
            .buffers
            .stack
            .get_mut(index)
            .ok_or_else(|| internal_error!("output buffer index after handler"))?;
        let original = std::mem::take(&mut buffer.bytes);
        buffer.started = true;
        if !matches!(&replacement, Ok(Some(_))) && buffer.callback.is_some() {
            buffer.disabled = true;
            buffer.capacity = 0;
        } else {
            buffer.processed = true;
        }
        if operation.removes() {
            self.output_writer.buffers.stack.pop();
        }
        match replacement {
            Ok(bytes) => {
                if !operation.discards() {
                    let bytes = bytes.unwrap_or(original);
                    self.out_below(index, &bytes);
                    if index == 0
                        && !bytes.is_empty()
                        && matches!(operation, Operation::Flush | Operation::EndFlush)
                    {
                        self.queue_response_flush();
                    }
                }
                deprecation
            }
            Err(error) => {
                if !operation.discards() && !matches!(operation, Operation::Shutdown) {
                    self.out_below(index, &original);
                }
                Err(error)
            }
        }
    }
}
