//! Reading about a prepared call: its parameters, its name, where an
//! argument goes.
//!
//! ```php
//! format(value: $x, ...$rest);
//! ```
//!
//! For the named argument `value`, the general path asks the call which
//! parameter is called `value`; for each element of `$rest`, which parameter
//! receives it and whether it takes a reference. A call to PHP code answers
//! from the function's bytecode, any other call from its [`Callee`]. Nothing
//! of this is stored per call.

use phera_extension_api::NativeError;

use crate::calls::call_stack::CallStack;
use crate::calls::call_state::{CallTarget, Callee};
use crate::calls::prepared_call::{
    CallDetails, CallFlags, Parameters, PreparedCall, PreparedCallee,
};
use crate::code_table::CodeTable;

/// A prepared call together with what is needed to read about it.
#[derive(Clone, Copy)]
pub(crate) struct CallView<'a> {
    pub(crate) call: PreparedCall,
    pub(crate) details: Option<&'a CallDetails>,
    code: &'a CodeTable,
}

/// Where the arguments of a prepared call are: one register per parameter
/// from `base`, and surplus arguments after the callee's own registers.
#[derive(Clone, Copy)]
pub(crate) struct ArgumentSlots {
    pub(crate) base: usize,
    size: usize,
    parameters: usize,
}

impl ArgumentSlots {
    /// The register that receives argument `index`.
    pub(crate) fn slot(&self, index: usize) -> usize {
        match index < self.parameters {
            true => self.base + index,
            false => self.base + self.size + index - self.parameters,
        }
    }
}

impl<'a> CallView<'a> {
    /// The innermost prepared call.
    pub(crate) fn top(code: &'a CodeTable, stack: &'a CallStack) -> Result<Self, NativeError> {
        Ok(Self::new(code, *stack.top_call()?, stack.top_details()))
    }

    pub(crate) fn new(
        code: &'a CodeTable,
        call: PreparedCall,
        details: Option<&'a CallDetails>,
    ) -> Self {
        Self {
            call,
            details,
            code,
        }
    }

    /// The description of a general callee.
    pub(crate) fn callee(&self) -> Option<&'a Callee> {
        self.details?.callee.as_ref()
    }

    /// The callee's parameters.
    pub(crate) fn parameters(&self) -> Parameters<'a> {
        match (self.call.callee, self.callee()) {
            (PreparedCallee::Php(function), _) => Parameters::Php {
                function: self.code.function(function),
                unit: self.code.unit(function),
            },
            (PreparedCallee::General, Some(callee)) => Parameters::Listed(&callee.parameters),
            (
                PreparedCallee::General | PreparedCallee::Native(_) | PreparedCallee::Constructed,
                _,
            ) => Parameters::Listed(&[]),
        }
    }

    /// The callee's name, as messages spell it.
    pub(crate) fn name(&self) -> &'a str {
        if self.call.flags.contains(CallFlags::INVOKE) {
            return "Closure::__invoke";
        }
        match (self.call.callee, self.callee()) {
            (PreparedCallee::Php(function), _) => &self.code.function(function).name,
            (PreparedCallee::General, Some(callee)) => &callee.name,
            // A call with no parameters is never named in a message.
            (
                PreparedCallee::General | PreparedCallee::Native(_) | PreparedCallee::Constructed,
                _,
            ) => "",
        }
    }

    /// Whether the callee is `extract()`, whose first argument is taken by
    /// reference when it is a variable.
    pub(crate) fn is_extract(&self) -> bool {
        self.callee().is_some_and(|callee| {
            matches!(
                callee.target,
                CallTarget::Intrinsic(crate::intrinsics::Intrinsic::Extract)
            )
        })
    }

    /// Where the call's arguments are.
    pub(crate) fn slots(&self) -> ArgumentSlots {
        let size = match (self.call.callee, self.callee()) {
            (PreparedCallee::Php(function), _) => {
                crate::calls::pending::php_window(self.code, function)
            }
            (PreparedCallee::General, Some(callee)) => {
                crate::calls::pending::general_window(callee)
            }
            (
                PreparedCallee::General | PreparedCallee::Native(_) | PreparedCallee::Constructed,
                _,
            ) => 1,
        };
        ArgumentSlots {
            base: self.call.base(),
            size,
            parameters: self.parameters().len(),
        }
    }

    /// The position the next argument goes to: the next one, or for a named
    /// argument its parameter's (or past the last for a variadic one).
    pub(crate) fn argument_index(&self) -> Result<usize, NativeError> {
        let sent = self.call.sent as usize;
        let Some(name) = self.details.and_then(|details| details.named.as_ref()) else {
            return Ok(sent);
        };
        let parameters = self.parameters();
        if let Some(index) = parameters.position_of(name.as_bytes()) {
            return Ok(index);
        }
        if let Some(index) = parameters.variadic() {
            return Ok(sent.max(index));
        }
        Err(NativeError::error(format!(
            "Unknown named parameter ${}",
            name.to_string_lossy()
        )))
    }
}
