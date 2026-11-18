//! `ob_flush()` sends a transformed chunk to the next outer buffer. An
//! output handler returning `false` passes the original bytes through and
//! is disabled for subsequent writes.

use phera_extension_api::{Context, NativeError, NativeResult, Unwind};
use phera_value::{Value, to_php_string};

use crate::{Vm, error_traces::DeclarationFailure};

use super::{CLEANABLE, FLUSHABLE, OutputFunction, REMOVABLE, Running};

#[derive(Clone, Copy)]
pub(crate) enum Operation {
    Chunk,
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
            Self::Chunk => 0,
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
    pub(crate) fn check_output_handler(&mut self, function: &str) -> Result<(), NativeError> {
        if self.output_writer.buffers.running.is_none() {
            return Ok(());
        }
        self.output_writer.buffers.lock_broken = true;
        let location = self.location();
        Err(NativeError::Unwind(Unwind(Box::new(DeclarationFailure {
            message: format!(
                "{function}(): Cannot use output buffering in output buffering display handlers"
            ),
            file: location.file,
            line: location.line,
        }))))
    }

    pub(crate) async fn change_buffer(
        &mut self,
        function: OutputFunction,
        operation: Operation,
    ) -> NativeResult {
        self.check_output_handler(function.name())?;
        let stack = &self.output_writer.buffers.stack;
        let Some((index, buffer)) = stack.iter().enumerate().next_back() else {
            if let Some(message) = operation.empty_notice() {
                self.notice(format!("{}(): {message}", function.name()));
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
            self.notice(format!(
                "{}(): Failed to {action} buffer of {name} ({index})",
                function.name()
            ));
            if matches!(operation, Operation::GetClean | Operation::GetFlush) {
                self.notice(format!(
                    "{}(): Failed to delete buffer of {name} ({index})",
                    function.name()
                ));
            }
            return Ok(captured.unwrap_or(Value::Bool(false)));
        }
        self.apply_buffer(index, operation).await?;
        self.drain_output().await?;
        Ok(captured.unwrap_or(Value::Bool(true)))
    }

    pub(crate) async fn drain_output(&mut self) -> Result<(), NativeError> {
        while let Some(index) = self.output_writer.buffers.pending.take() {
            self.apply_buffer(index, Operation::Chunk).await?;
        }
        if self.headers_pending() {
            self.finish_headers().await?;
        }
        Ok(())
    }

    pub(crate) async fn finish_buffers(&mut self) {
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
        let result = if let Some(callback) = callback {
            let arguments = [Value::from(buffer.bytes.as_slice()), Value::Int(mode)];
            self.output_writer.buffers.running = Some(Running {
                index,
                produced_output: false,
            });
            let result = self.invoke_resolved_callable(callback, arguments).await;
            let buffers = &mut self.output_writer.buffers;
            let running = buffers.running.take();
            if std::mem::take(&mut buffers.lock_broken) {
                buffers.stack.clear();
                buffers.pending = None;
                return result.map(|_| ());
            }
            if running.is_some_and(|running| running.produced_output) && result.is_ok() {
                Err(NativeError::throw(
                    "Phera\\NotSupportedException",
                    "Producing output from user output handlers is deprecated since PHP 8.5",
                ))
            } else {
                result
            }
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
                    self.out_below(index, &bytes.unwrap_or(original));
                }
                Ok(())
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
