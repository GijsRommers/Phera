//! Native signatures validate handler intrinsic counts and scalar argument types.

use crate::error_handlers::HandlerFunction;
use phera_extension_api::{Param, ParamType};

/// Argument metadata validates handler intrinsics before mutating request state.
pub(crate) struct HandlerSignature {
    pub(crate) name: &'static str,
    pub(crate) parameters: &'static [Param],
}

impl HandlerFunction {
    /// Callable validation itself uses the VM resolver so private scopes are honored.
    pub(crate) fn signature(self) -> HandlerSignature {
        const CALLBACK: &[Param] = &[Param::new("callback", ParamType::Mixed)];
        const SET_ERROR: &[Param] = &[
            Param::new("callback", ParamType::Mixed),
            Param::new("error_levels", ParamType::Int).optional(),
        ];
        const TRIGGER: &[Param] = &[
            Param::new("message", ParamType::String),
            Param::new("error_level", ParamType::Int).optional(),
        ];
        match self {
            Self::SetException => HandlerSignature {
                name: "set_exception_handler",
                parameters: CALLBACK,
            },
            Self::SetError => HandlerSignature {
                name: "set_error_handler",
                parameters: SET_ERROR,
            },
            Self::RestoreException => HandlerSignature {
                name: "restore_exception_handler",
                parameters: &[],
            },
            Self::RestoreError => HandlerSignature {
                name: "restore_error_handler",
                parameters: &[],
            },
            Self::GetException => HandlerSignature {
                name: "get_exception_handler",
                parameters: &[],
            },
            Self::GetError => HandlerSignature {
                name: "get_error_handler",
                parameters: &[],
            },
            Self::TriggerError => HandlerSignature {
                name: "trigger_error",
                parameters: TRIGGER,
            },
        }
    }
}
