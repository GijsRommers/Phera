//! Resolved signatures and pending arguments describe a PHP call before it executes.

use std::rc::Rc;

/// A signature and target resolved once for a symbol in this request.
#[derive(Clone)]
pub(crate) struct Callee {
    pub(crate) argument_binding: crate::calls::callback_binding::ArgumentBinding,
    pub(crate) signature: crate::signatures::SignatureIndex,
    pub(crate) target: CallTarget,
}

/// Named argument routing and PHP parameter binding modifiers.
#[derive(Clone)]
pub(crate) struct ParameterSpec {
    pub(crate) name: String,
    /// Immutable declared type lets Send test its mask without reopening the function table.
    pub(crate) ty: Option<phera_bytecode::Type>,
    pub(crate) by_reference: bool,
    pub(crate) variadic: bool,
}

/// User bytecode, a native entry, or an intrinsic resolved at the call site.
#[derive(Clone)]
pub(crate) enum CallTarget {
    /// Bound methods retain lexical and called scopes throughout argument evaluation.
    Method {
        method: crate::classes::method_table::MethodAddress,
        context: crate::classes::MethodContext,
    },
    /// A class without a constructor still consumes sends and returns the allocated handle.
    Constructed(Rc<phera_value::Object>),
    Closure(Rc<phera_value::Object>),
    /// Compiled code ownership is shared without allocating a per-invocation callee record.
    User {
        function: crate::code_table::FunctionIndex,
    },
    Native(crate::signatures::NativeIndex),
    Intrinsic(crate::intrinsics::Intrinsic),
}

/// A failed fast type test must wait for Call, after all argument side effects.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeChecks {
    Complete,
    Deferred,
}

/// Once a named argument is seen, further unpacked integer keys are illegal.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgumentOrder {
    Positional,
    Named,
}

/// One unfinished call pairs its resolved target with its reserved value window.
pub(crate) struct PendingCall {
    pub(crate) owner: Option<Rc<phera_value::Object>>,
    pub(crate) callee: Callee,
    pub(crate) window: CallWindow,
}

/// The register layout survives target ownership moving into an executing frame.
pub(crate) struct CallWindow {
    pub(crate) base: usize,
    pub(crate) registers: usize,
    pub(crate) parameters: usize,
    pub(crate) supplied: usize,
    /// Named variadic entries do not contribute to func_num_args.
    pub(crate) argument_count: usize,
    pub(crate) checks: TypeChecks,
    pub(crate) named: Option<phera_value::PhpString>,
    pub(crate) order: ArgumentOrder,
    pub(crate) extra_named: Vec<(usize, phera_value::PhpString)>,
}

impl CallWindow {
    /// Surplus arguments live beyond locals, so they cannot overwrite the callee's code slots.
    pub(crate) fn slot(&self, index: usize) -> usize {
        self.base
            + if index < self.parameters {
                index
            } else {
                self.registers + index - self.parameters
            }
    }
}

impl PendingCall {
    /// Positional sends follow the high-water parameter index; names select declaration slots.
    pub(crate) fn argument_index(
        &self,
        signatures: &crate::signatures::Signatures,
    ) -> Result<usize, phera_extension_api::NativeError> {
        let Some(name) = &self.window.named else {
            return Ok(self.window.supplied);
        };
        if let Some(index) = signatures
            .get(self.callee.signature)
            .parameters
            .iter()
            .position(|parameter| {
                parameter.name.as_bytes() == name.as_bytes() && !parameter.variadic
            })
        {
            return Ok(index);
        }
        if let Some(index) = signatures
            .get(self.callee.signature)
            .parameters
            .iter()
            .position(|parameter| parameter.variadic)
        {
            return Ok(self.window.supplied.max(index));
        }
        Err(phera_extension_api::NativeError::error(format!(
            "Unknown named parameter ${}",
            name.to_string_lossy()
        )))
    }
}

impl ParameterSpec {
    /// User signatures are retained once, so sends need no unit or name-table lookup.
    pub(crate) fn user(
        unit: &phera_bytecode::CompiledUnit,
        parameter: &phera_bytecode::Parameter,
    ) -> Self {
        Self {
            name: unit.names[parameter.name.index()].clone(),
            ty: parameter.ty.map(|ty| unit.types[ty.index()].clone()),
            by_reference: parameter.by_reference,
            variadic: parameter.variadic,
        }
    }

    /// Native signatures have their own coercion policy at the extension boundary.
    pub(crate) fn native(parameter: &phera_extension_api::Param) -> Self {
        Self {
            name: parameter.name.into(),
            ty: None,
            by_reference: parameter.by_ref,
            variadic: parameter.variadic,
        }
    }
}
