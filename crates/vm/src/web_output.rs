//! `flush()` hands pending bytes to HTTP. An ordinary response stays buffered
//! in the VM until the script and its shutdown functions have finished.

use phera_extension_api::{LocalFuture, NativeError, NativeResult, RequestState, Unwind};
use phera_value::Value;

use crate::Vm;

/// A web SAPI can publish a response head and then send bytes as PHP runs.
pub trait WebOutput {
    /// Observe whether it is still possible to write to the client.
    fn is_disconnected(&self) -> bool;

    /// Start a streamed body, after PHP has prepared the response head.
    fn start(&mut self, state: &RequestState);

    /// Send bytes, waiting for room in the transport's bounded queue.
    fn write(&mut self, bytes: Vec<u8>) -> LocalFuture<'_, bool>;

    /// Close a streamed body after PHP sends its last chunk.
    fn end_stream(&mut self);

    /// Send a complete buffered response before the PHP request ends.
    fn finish_buffered(&mut self, state: &RequestState, bytes: Vec<u8>) -> LocalFuture<'_, bool>;
}

#[derive(PartialEq, Eq)]
enum Phase {
    Buffered,
    Streaming,
    Finished,
    Aborted,
}

pub(crate) struct ResponseOutput {
    transport: Box<dyn WebOutput>,
    phase: Phase,
    pub(crate) pending: bool,
}

impl ResponseOutput {
    pub(crate) fn is_ended(&self) -> bool {
        matches!(self.phase, Phase::Finished | Phase::Aborted)
    }

    pub(crate) fn writes_immediately(&self) -> bool {
        self.phase == Phase::Streaming || self.transport.is_disconnected()
    }
}

impl Vm {
    pub fn set_web_output(&mut self, transport: Box<dyn WebOutput>) {
        self.output_writer.response = Some(ResponseOutput {
            transport,
            phase: Phase::Buffered,
            pending: false,
        });
    }

    pub(crate) fn queue_response_flush(&mut self) {
        if let Some(response) = &mut self.output_writer.response
            && !response.is_ended()
        {
            response.pending = true;
            self.pending_callbacks = true;
        }
    }

    pub(crate) fn client_aborted(&self) -> bool {
        self.output_writer
            .response
            .as_ref()
            .is_some_and(|response| response.phase == Phase::Aborted)
    }

    pub(crate) async fn flush_response_body(&mut self) -> Result<(), NativeError> {
        let Some(response) = &mut self.output_writer.response else {
            return Ok(());
        };
        response.pending = false;
        if response.is_ended() {
            return Ok(());
        }
        if response.transport.is_disconnected() {
            return self.abort_response();
        }
        if response.phase == Phase::Buffered {
            response.transport.start(&self.state);
            response.phase = Phase::Streaming;
            // Give the connection its response before resuming PHP, even
            // for a headers-only flush or a chunk that fits the queue.
            tokio::task::yield_now().await;
        }
        let body = std::mem::take(&mut self.output);
        if !body.is_empty() && !response.transport.write(body).await {
            return self.abort_response();
        }
        Ok(())
    }

    fn abort_response(&mut self) -> Result<(), NativeError> {
        if let Some(response) = &mut self.output_writer.response {
            response.phase = Phase::Aborted;
        }
        self.output = Vec::new();
        if self.ini_enabled(b"ignore_user_abort") {
            Ok(())
        } else {
            Err(NativeError::Unwind(Unwind(Box::new(
                crate::request_end::FatalStop,
            ))))
        }
    }

    pub(crate) async fn finish_response_early(&mut self, function: &'static str) -> NativeResult {
        if self
            .output_writer
            .response
            .as_ref()
            .is_none_or(ResponseOutput::is_ended)
        {
            return Ok(Value::Bool(false));
        }
        let buffered = self.finish_response_buffers(function).await;
        if buffered.as_ref().is_err_and(|error| {
            crate::request_end::ScriptEnd::of(error) == crate::request_end::ScriptEnd::Fatal
        }) {
            return buffered.map(|()| Value::Bool(false));
        }
        self.commit_response_headers().await?;
        if self.is_streaming_response() {
            self.flush_response_body().await?;
            self.finish_stream();
        } else if self
            .output_writer
            .response
            .as_ref()
            .is_some_and(|response| response.transport.is_disconnected())
        {
            self.abort_response()?;
            return Ok(Value::Bool(false));
        } else if let Some(response) = &mut self.output_writer.response
            && !response.is_ended()
        {
            response.phase = Phase::Finished;
            let sent = response
                .transport
                .finish_buffered(&self.state, std::mem::take(&mut self.output))
                .await;
            if !sent {
                self.abort_response()?;
                return Ok(Value::Bool(false));
            }
        }
        // The response can end even if the script's next operation is CPU work.
        tokio::task::yield_now().await;
        buffered.map(|()| Value::Bool(true))
    }

    pub(crate) async fn complete_streamed_response(&mut self) {
        if self.is_streaming_response() {
            if let Err(error) = self.flush_response_body().await {
                self.unwind(error);
            }
            self.finish_stream();
        }
    }

    fn is_streaming_response(&self) -> bool {
        self.output_writer
            .response
            .as_ref()
            .is_some_and(|response| response.phase == Phase::Streaming)
    }

    fn finish_stream(&mut self) {
        if let Some(response) = &mut self.output_writer.response
            && response.phase == Phase::Streaming
        {
            response.phase = Phase::Finished;
            response.transport.end_stream();
        }
    }
}
