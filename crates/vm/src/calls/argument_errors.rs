//! Argument-introspection failures are ordinary catchable PHP Error and ValueError objects.

use crate::Vm;
use phera_extension_api::{NativeError, NativeResult, Param, ParamType};
use phera_value::{PhpArray, Value};
use std::rc::Rc;

/// These native functions inspect the active PHP frame rather than their own native arguments.
#[derive(Clone, Copy)]
pub(crate) enum ArgumentFunction {
    Count,
    All,
    One,
}

impl ArgumentFunction {
    /// Error messages and trace frames use the native spelling PHP exposes.
    fn name(self) -> &'static str {
        match self {
            Self::Count => "func_num_args",
            Self::All => "func_get_args",
            Self::One => "func_get_arg",
        }
    }
}

impl Vm {
    /// Validate native arguments before checking whether a PHP function frame exists.
    pub(crate) fn argument_intrinsic(
        &mut self,
        function: ArgumentFunction,
        mut arguments: Vec<Value>,
        dynamic: bool,
    ) -> NativeResult {
        const POSITION: &[Param] = &[Param::new("position", ParamType::Int)];
        let parameters = if matches!(function, ArgumentFunction::One) {
            POSITION
        } else {
            &[]
        };
        let result = self
            .prepare_call_with_trace(function.name(), parameters, &mut arguments)
            .and_then(|()| self.frame_arguments_result(function, &arguments, dynamic));
        if result.is_err() {
            self.failed_native = Some(crate::error_traces::NativeTrace::new(
                function.name(),
                &arguments,
            ));
        }
        result
    }

    /// Negative positions fail before the global-scope check, as native parameter parsing does.
    fn frame_arguments_result(
        &self,
        function: ArgumentFunction,
        arguments: &[Value],
        dynamic: bool,
    ) -> NativeResult {
        let position = if let Some(Value::Int(position)) = arguments.first() {
            *position
        } else {
            0
        };
        if matches!(function, ArgumentFunction::One) && position < 0 {
            return Err(NativeError::value_error(
                "func_get_arg(): Argument #1 ($position) must be greater than or equal to 0",
            ));
        }
        let frame = self
            .current()
            .filter(|frame| frame.function_id().is_some())
            .ok_or_else(|| {
                NativeError::error(match function {
                    ArgumentFunction::Count => {
                        "func_num_args() must be called from a function context"
                    }
                    ArgumentFunction::All => {
                        "func_get_args() cannot be called from the global scope"
                    }
                    ArgumentFunction::One => {
                        "func_get_arg() cannot be called from the global scope"
                    }
                })
            })?;
        if dynamic {
            return Err(NativeError::error(format!(
                "Cannot call {}() dynamically",
                function.name()
            )));
        }
        match function {
            ArgumentFunction::Count => Ok(Value::Int(frame.argument_count() as i64)),
            ArgumentFunction::All => self.argument_array(frame, 0),
            ArgumentFunction::One => (position < frame.argument_count() as i64)
                .then(|| frame.argument(&self.registers, position as usize))
                .ok_or_else(|| {
                    NativeError::value_error(
                        "func_get_arg(): Argument #1 ($position) must be less than the number \
                     of the arguments passed to the currently executed function",
                    )
                }),
        }
    }

    /// `array_slice(func_get_args(), $start)` starts reading at the first wanted slot.
    pub(crate) fn argument_array(
        &self,
        frame: crate::calls::frame_view::FrameView<'_>,
        start: usize,
    ) -> NativeResult {
        let mut result = PhpArray::new();
        for index in start..frame.argument_count() {
            result.push(frame.argument(&self.registers, index))?;
        }
        Ok(Value::Array(Rc::new(result)))
    }
}

impl Vm {
    /// Validate the complete native call before changing PHP's request reporting mask.
    pub(crate) fn error_reporting_intrinsic(&mut self, mut arguments: Vec<Value>) -> NativeResult {
        const PARAMETERS: &[Param] = &[Param::new("error_level", ParamType::Int)
            .nullable()
            .optional()];
        if let Err(error) =
            self.prepare_call_with_trace("error_reporting", PARAMETERS, &mut arguments)
        {
            self.failed_native = Some(crate::error_traces::NativeTrace::new(
                "error_reporting",
                &arguments,
            ));
            return Err(error);
        }
        let previous = self.error_handlers.reporting;
        if let Some(Value::Int(level)) = arguments.first() {
            self.set_error_reporting(*level);
        }
        Ok(Value::Int(previous))
    }
}
