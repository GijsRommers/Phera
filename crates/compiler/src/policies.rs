//! Explicit policies for declaration timing and PHP name resolution.

/// Whether a declaration exists before execution reaches its statement.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeclarationTiming {
    /// Top-level functions are available before any statements execute.
    Hoisted,
    /// Conditional and nested declarations are installed when executed.
    AtExecution,
    /// An initializer reached only through declaration metadata.
    Hidden,
}

pub(crate) use phera_bytecode::reflection::NameKind;

/// Whether foreach copies values or binds the loop variable to each element.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum IterationBinding {
    Value,
    Reference,
}
