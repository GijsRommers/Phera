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

/// PHP keeps class, function and constant aliases in separate namespaces.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum NameKind {
    Class,
    Function,
    Constant,
}

/// Whether foreach copies values or binds the loop variable to each element.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum IterationBinding {
    Value,
    Reference,
}
