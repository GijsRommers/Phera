//! PHP output instruction handlers, separate from dispatch.

use phera_bytecode::Reg;
use phera_extension_api::{LocalFuture, NativeError, OutputDiagnostics, RequestState};
use phera_value::{Value, to_php_string};

use crate::{OutputSink, Vm, diagnostics::SourceLocation, output_buffer::Buffers};

impl Vm {
    #[cold]
    pub(super) fn drain_output_diagnostics(&mut self) -> LocalFuture<'_, OutputDiagnostics> {
        Box::pin(async move {
            use crate::request_end::ScriptEnd;

            match self.drain_error_handlers().await {
                Ok(()) => OutputDiagnostics::Ready,
                Err(error) if ScriptEnd::of(&error) == ScriptEnd::Fatal => {
                    OutputDiagnostics::Stop(error)
                }
                Err(error) => OutputDiagnostics::WriteThenUnwind(error),
            }
        })
    }

    /// Implements PHP Echo semantics outside the dispatch loop.
    pub(crate) fn echo_value(&mut self, source: Reg) -> Result<(), NativeError> {
        let text = match &*self.operand(source) {
            Value::InternedStr(text) => Some(text.to_php_string()),
            Value::String(text) => Some(text.clone()),
            _ => None,
        };
        if let Some(text) = text {
            if self.diagnostic_reporter.has_pending() {
                self.flush_operator_diagnostics();
            }
            self.out(text.as_bytes());
            return Ok(());
        }
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let text = to_php_string(&diagnostics.vm.operand(source), &mut diagnostics.buffer)?;
        drop(diagnostics);
        self.out(text.as_bytes());
        Ok(())
    }
}

/// Output forwarding is separate from the public capture buffer required by embedders.
pub(crate) struct Output {
    sink: Option<Box<dyn OutputSink>>,
    pub(crate) buffers: Buffers,
    implicit_flush: bool,
    prepare_headers: Option<fn(&mut RequestState)>,
    set_error_status: Option<fn(&mut RequestState)>,
    web: bool,
    pub(crate) web_diagnostics: Option<crate::diagnostic_output::WebDiagnostics>,
    pub(crate) origin: Option<SourceLocation>,
    queued_origin: Option<SourceLocation>,
    pub(crate) chunk_caller: Option<&'static str>,
    queued: Vec<u8>,
    pub(crate) debugger: Option<std::rc::Rc<crate::debugger::Streams>>,
}

impl Output {
    pub(crate) fn new() -> Self {
        Self {
            sink: None,
            buffers: Buffers::default(),
            implicit_flush: false,
            prepare_headers: None,
            set_error_status: None,
            web: false,
            web_diagnostics: None,
            origin: None,
            queued_origin: None,
            chunk_caller: None,
            queued: Vec::new(),
            debugger: None,
        }
    }

    pub(crate) fn set_sink(&mut self, sink: Box<dyn OutputSink>) {
        self.sink = Some(sink);
    }

    pub(crate) fn set_implicit_flush(&mut self, enable: bool) {
        self.implicit_flush = enable;
    }

    /// Forward first, then retain the bytes, matching the existing sink observation order.
    pub(crate) fn write(&mut self, captured: &mut Vec<u8>, bytes: &[u8]) {
        if self
            .debugger
            .as_ref()
            .is_some_and(|debugger| !debugger.buffer(crate::debugger::Stream::Stdout, bytes))
        {
            return;
        }
        if let Some(sink) = &mut self.sink {
            sink.write(bytes);
            if self.implicit_flush {
                sink.flush();
            }
        }
        captured.extend_from_slice(bytes);
    }
}

impl Vm {
    /// Configures the response preparation step before the first web output.
    pub fn prepare_web_headers_with(
        &mut self,
        prepare: fn(&mut RequestState),
        set_error_status: fn(&mut RequestState),
    ) {
        self.output_writer.prepare_headers = Some(prepare);
        self.output_writer.set_error_status = Some(set_error_status);
        self.output_writer.web = true;
    }

    pub(crate) fn set_hidden_fatal_status(&mut self) {
        if self.output_writer.web && !self.display_errors_enabled() {
            self.set_unsent_error_status();
        }
    }

    pub(super) fn set_unsent_error_status(&mut self) {
        if self.output_writer.origin.is_none()
            && let Some(set_status) = self.output_writer.set_error_status
        {
            set_status(&mut self.state);
        }
    }

    fn prepare_response_headers(&mut self) {
        if let Some(prepare) = self.output_writer.prepare_headers.take() {
            prepare(&mut self.state);
        }
    }

    /// Sends bytes written by a PHP script through its request output.
    pub fn out(&mut self, bytes: &[u8]) {
        self.out_below(self.output_writer.buffers.len(), bytes);
    }

    pub(crate) fn out_below(&mut self, depth: usize, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if depth != 0 && self.output_writer.buffers.write(depth, bytes) {
            self.pending_callbacks |= self.output_writer.buffers.pending.is_some();
            return;
        }
        if self.output_writer.origin.is_none() {
            self.begin_output(bytes);
        } else {
            self.output_writer.write(&mut self.output, bytes);
        }
    }

    #[cold]
    fn begin_output(&mut self, bytes: &[u8]) {
        if self.request_callbacks.output.has_headers() {
            if self.output_writer.queued_origin.is_none() {
                self.output_writer.queued_origin = Some(self.location());
                self.pending_callbacks = true;
            }
            self.output_writer.queued.extend_from_slice(bytes);
        } else {
            self.prepare_response_headers();
            self.output_writer.origin = Some(self.location());
            self.output_writer.write(&mut self.output, bytes);
        }
    }

    pub(crate) fn output_pending(&self) -> bool {
        self.headers_pending() || self.output_writer.buffers.pending.is_some()
    }

    pub(crate) fn headers_pending(&self) -> bool {
        self.output_writer.queued_origin.is_some()
    }

    pub(crate) async fn flush_transport(&mut self) -> Result<(), NativeError> {
        if self.output_writer.web && self.output_writer.origin.is_none() {
            if self.output_writer.queued_origin.is_none() {
                self.output_writer.queued_origin = Some(SourceLocation {
                    file: String::new(),
                    line: 0,
                });
            }
            self.finish_headers().await?;
        }
        if let Some(sink) = &mut self.output_writer.sink {
            sink.flush();
        }
        Ok(())
    }

    pub(crate) async fn finish_headers(&mut self) -> Result<(), NativeError> {
        self.finish_headers_at::<false>().await
    }

    pub(crate) async fn finish_request_headers(&mut self) {
        if let Err(error) = self.finish_headers_at::<true>().await {
            self.unwind(error);
        }
    }

    async fn finish_headers_at<const REQUEST_END: bool>(&mut self) -> Result<(), NativeError> {
        self.prepare_response_headers();
        let origin = self.output_writer.queued_origin.take();
        self.refresh_pending_callbacks();
        let bytes = std::mem::take(&mut self.output_writer.queued);
        if matches!(
            self.request_callbacks.output,
            crate::request_callbacks::OutputCallbacks::None
        ) {
            self.publish_output(origin, &bytes);
            return Ok(());
        }
        let callbacks = std::mem::take(&mut self.request_callbacks.output);
        let (headers, finalizer) = callbacks.into_callbacks();
        if let Some(finalizer) = finalizer {
            self.request_callbacks.output.set_finalizer(finalizer);
        }
        let result = match headers {
            Some(callback) => self
                .invoke_resolved_callable(callback, [])
                .await
                .map(|_| ()),
            None => Ok(()),
        };
        self.publish_output(origin, &bytes);
        if !REQUEST_END {
            return result;
        }
        if let Err(error) = result {
            self.unwind(error);
        }
        let (_, finalizer) = std::mem::take(&mut self.request_callbacks.output).into_callbacks();
        if let Some(finalizer) = finalizer
            && let Err(error) = finalizer(self, Vec::new()).await
        {
            self.report_uncaught(error).await;
        }
        Ok(())
    }

    fn publish_output(&mut self, origin: Option<SourceLocation>, bytes: &[u8]) {
        if self.output_writer.origin.is_none() || origin.is_some() {
            self.output_writer.origin = origin.or_else(|| Some(self.location()));
        }
        if !bytes.is_empty() {
            self.output_writer.write(&mut self.output, bytes);
        }
    }
}

/// A fork writes to the sink its request gives it; the buffers are the
/// snapshot's, emptied (see `crate::output_buffer`).
impl crate::snapshot::ForkCopy for Output {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        Output {
            buffers: self.buffers.fork_copy(fork),
            implicit_flush: self.implicit_flush,
            prepare_headers: self.prepare_headers,
            set_error_status: self.set_error_status,
            web: self.web,
            ..Output::new()
        }
    }
}
