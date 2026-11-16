//! PHP output instruction handlers, separate from dispatch.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::to_php_string;

use crate::{OutputSink, Vm};

impl Vm {
    /// Implements PHP Echo semantics outside the dispatch loop.
    pub(crate) fn echo_value(&mut self, source: Reg) -> Result<(), NativeError> {
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
}

impl Output {
    pub(crate) fn new() -> Self {
        Self { sink: None }
    }

    pub(crate) fn set_sink(&mut self, sink: Box<dyn OutputSink>) {
        self.sink = Some(sink);
    }

    /// Forward first, then retain the bytes, matching the existing sink observation order.
    pub(crate) fn write(&mut self, captured: &mut Vec<u8>, bytes: &[u8]) {
        if let Some(sink) = &mut self.sink {
            sink.write(bytes);
        }
        captured.extend_from_slice(bytes);
    }
}
