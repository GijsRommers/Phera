//! An error created in `$pdo->query($sql)` starts with that method's call frame.
//! An exception a PHP callback already created keeps the trace it captured there.

use phera_extension_api::{NativeError, NativeResult};
use phera_value::Value;

use crate::{Vm, classes::Method, error_traces::NativeTrace};

pub(crate) enum NativeFrame<'a> {
    Function(&'a str),
    Method(&'a Method),
}

pub(super) enum NativeArguments {
    Empty,
    One(Value),
    Two([Value; 2]),
    Many(Vec<Value>),
}

impl NativeArguments {
    pub(super) fn as_slice(&self) -> &[Value] {
        match self {
            Self::Empty => &[],
            Self::One(value) => std::slice::from_ref(value),
            Self::Two(values) => values,
            Self::Many(values) => values,
        }
    }
}

impl Vm {
    /// A native error only supplies a frame when the VM still has to create its
    /// Throwable. A callback's existing Throwable and VM continuations carry
    /// their own provenance.
    #[cold]
    pub(crate) fn needs_native_failure_frame(&self, error: &NativeError) -> bool {
        self.failed_native.is_none() && error.is_exception_request()
    }

    #[cold]
    pub(crate) fn record_native_failure(
        &mut self,
        error: &NativeError,
        frame: NativeFrame<'_>,
        arguments: &[Value],
    ) {
        if !self.needs_native_failure_frame(error) {
            return;
        }
        let trace = match frame {
            NativeFrame::Function(name) => self.native_function_trace(name, arguments),
            NativeFrame::Method(method) => self.native_method_trace(method, arguments),
        };
        self.failed_native = Some(trace);
    }

    #[cold]
    pub(crate) fn native_method_trace(
        &mut self,
        method: &Method,
        arguments: &[Value],
    ) -> NativeTrace {
        let name = NativeTrace::method_name(method);
        match method.body {
            crate::classes::MethodBody::Native(entry) => {
                self.native_parameter_trace(&name, entry.params, arguments)
            }
            _ => NativeTrace::new(&name, arguments),
        }
    }

    /// Async handlers own their arguments. Keep small calls in the VM's future
    /// and borrow a buffer for larger calls.
    pub(super) fn retain_native_arguments(&mut self, arguments: &[Value]) -> NativeArguments {
        match arguments {
            [] => NativeArguments::Empty,
            [argument] => NativeArguments::One(argument.clone()),
            [first, second] => NativeArguments::Two([first.clone(), second.clone()]),
            arguments => {
                let mut retained = self.call_stack.native_arguments();
                retained.extend_from_slice(arguments);
                NativeArguments::Many(retained)
            }
        }
    }

    pub(super) fn finish_native_future(
        &mut self,
        result: NativeResult,
        frame: NativeFrame<'_>,
        arguments: NativeArguments,
    ) -> NativeResult {
        if let Err(error) = &result {
            self.record_native_failure(error, frame, arguments.as_slice());
        }
        if let NativeArguments::Many(arguments) = arguments {
            self.call_stack.recycle_native_arguments(arguments);
        }
        result
    }
}
