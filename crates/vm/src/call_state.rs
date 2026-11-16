//! Resolved signatures and pending arguments describe a PHP call before it executes.

use std::rc::Rc;

use phera_extension_api::FunctionEntry;
use phera_value::Value;

/// A signature and target resolved once for a symbol in this request.
pub(crate) struct Callee {
    pub(crate) name: String,
    pub(crate) parameters: Vec<ParameterSpec>,
    pub(crate) target: CallTarget,
}

/// Named argument routing and PHP parameter binding modifiers.
pub(crate) struct ParameterSpec {
    pub(crate) name: String,
    pub(crate) by_reference: bool,
    pub(crate) variadic: bool,
}

/// User bytecode, a native entry, or an intrinsic resolved at the call site.
pub(crate) enum CallTarget {
    User(crate::function_table::FunctionRef),
    Native(FunctionEntry),
    Intrinsic(crate::intrinsics::Intrinsic),
}

/// An unfinished call; nested argument calls push another entry.
pub(crate) struct PendingCall {
    pub(crate) callee: Rc<Callee>,
    pub(crate) arguments: Vec<Option<Value>>,
    pub(crate) named: Option<String>,
}

impl PendingCall {
    /// Named sends select a signature slot; positional sends follow prepared arguments.
    pub(crate) fn argument_index(&self) -> Result<usize, phera_extension_api::NativeError> {
        if let Some(named) = &self.named {
            self.callee
                .parameters
                .iter()
                .position(|parameter| &parameter.name == named)
                .ok_or_else(|| {
                    phera_extension_api::NativeError::error(format!(
                        "Unknown named parameter ${named}"
                    ))
                })
        } else {
            Ok(self.arguments.len())
        }
    }
}

/// Signature metadata is built together with the resolved execution target.
pub(crate) struct ResolvedSignature {
    pub(crate) target: CallTarget,
    pub(crate) parameters: Vec<ParameterSpec>,
}
