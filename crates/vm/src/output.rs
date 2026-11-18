//! PHP output instruction handlers, separate from dispatch.

use phera_bytecode::Reg;
use phera_extension_api::{NativeError, RequestState};
use phera_value::to_php_string;

use crate::{OutputSink, Vm, diagnostics::SourceLocation, output_buffer::Buffers};

impl Vm {
    /// Implements PHP Echo semantics outside the dispatch loop.
    pub(crate) fn echo_value(&mut self, source: Reg) -> Result<(), NativeError> {
        let throwable_text = match &*self.operand(source) {
            phera_value::Value::Object(object)
                if self
                    .classes
                    .for_object(object)
                    .is_some_and(|class| class.is_throwable) =>
            {
                self.throwable_method_result("__toString", object)
                    .and_then(|value| match value {
                        phera_value::Value::String(text) => Some(text),
                        _ => None,
                    })
            }
            _ => None,
        };
        if let Some(text) = throwable_text {
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
    web: bool,
    pub(crate) origin: Option<SourceLocation>,
    queued_origin: Option<SourceLocation>,
    queued: Vec<u8>,
}

impl Output {
    pub(crate) fn new() -> Self {
        Self {
            sink: None,
            buffers: Buffers::default(),
            implicit_flush: false,
            prepare_headers: None,
            web: false,
            origin: None,
            queued_origin: None,
            queued: Vec::new(),
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
    pub fn prepare_web_headers_with(&mut self, prepare: fn(&mut RequestState)) {
        self.output_writer.prepare_headers = Some(prepare);
        self.output_writer.web = true;
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
        if bytes.is_empty() || (depth != 0 && self.output_writer.buffers.write(depth, bytes)) {
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
        if self.request_callbacks.headers.is_some() {
            if self.output_writer.queued_origin.is_none() {
                self.output_writer.queued_origin = Some(self.location());
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
        self.prepare_response_headers();
        let origin = self.output_writer.queued_origin.take();
        let bytes = std::mem::take(&mut self.output_writer.queued);
        if let Some(callback) = self.request_callbacks.headers.take() {
            self.invoke_resolved_callable(callback, []).await?;
        }
        if self.output_writer.origin.is_none() {
            self.output_writer.origin = origin.or_else(|| Some(self.location()));
        }
        if !bytes.is_empty() {
            self.output_writer.write(&mut self.output, &bytes);
        }
        Ok(())
    }
}
