//! `flush()` hands pending bytes to HTTP. An ordinary response stays buffered
//! in the VM until the script and its shutdown functions have finished.

use std::rc::Rc;

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

/// Opens an HTTP output transport only when PHP first needs to send bytes.
pub trait WebOutputFactory {
    fn open(&self, state: &RequestState) -> Option<Box<dyn WebOutput>>;
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
}

impl ResponseOutput {
    pub(crate) fn is_ended(&self) -> bool {
        matches!(self.phase, Phase::Finished | Phase::Aborted)
    }

    pub(crate) fn is_streaming(&self) -> bool {
        self.phase == Phase::Streaming
    }
}

impl Vm {
    pub fn set_web_output_factory(&mut self, factory: Rc<dyn WebOutputFactory>) {
        self.output_writer.set_factory(factory);
    }

    fn open_response_transport(&mut self) {
        if self.output_writer.response().is_none()
            && let Some(factory) = self.output_writer.factory.take()
            && let Some(transport) = factory.open(&self.state)
        {
            self.output_writer.set_response(ResponseOutput {
                transport,
                phase: Phase::Buffered,
            });
        }
    }

    pub(crate) fn queue_response_flush(&mut self) {
        if self.output_writer.factory.is_some()
            || self
                .output_writer
                .response()
                .is_some_and(|response| !response.is_ended())
        {
            self.output_writer.response_pending = true;
            self.pending_callbacks = true;
        }
    }

    pub(crate) fn client_aborted(&self) -> bool {
        self.output_writer
            .response()
            .is_some_and(|response| response.phase == Phase::Aborted)
    }

    pub(crate) async fn flush_response_body(&mut self) -> Result<(), NativeError> {
        self.output_writer.response_pending = false;
        self.open_response_transport();
        let Some(response) = self.output_writer.response_mut() else {
            return Ok(());
        };
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
        if let Some(response) = self.output_writer.response_mut() {
            response.phase = Phase::Aborted;
        }
        self.output = Vec::new();
        if self.ini_enabled(b"ignore_user_abort") {
            Ok(())
        } else {
            self.release_frames_for_exit();
            Err(NativeError::Unwind(Unwind(Box::new(
                crate::request_end::Exit,
            ))))
        }
    }

    pub(crate) async fn finish_response_early(&mut self, function: &'static str) -> NativeResult {
        self.open_response_transport();
        if self
            .output_writer
            .response()
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
            .response()
            .is_some_and(|response| response.transport.is_disconnected())
        {
            self.abort_response()?;
            return Ok(Value::Bool(false));
        } else if let Some(response) = self.output_writer.response_mut()
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
            .response()
            .is_some_and(ResponseOutput::is_streaming)
    }

    fn finish_stream(&mut self) {
        if let Some(response) = self.output_writer.response_mut()
            && response.phase == Phase::Streaming
        {
            response.phase = Phase::Finished;
            response.transport.end_stream();
        }
    }
}
