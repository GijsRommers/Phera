//! Explicit policies distinguish PHP reads, assignments and reference rebinding.

/// Undefined slots warn for ordinary reads, but isset-style reads stay silent.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Missing {
    Warn,
    Silent,
}

/// Writing an ordinary value preserves a destination alias; binding replaces it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Binding {
    ByValue,
    ByReference,
}

/// Dynamic writes may introduce a name; reads and unset only look it up.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Creation {
    Existing,
    Create,
}

/// Nested path reconstruction distinguishes removal from assignment and rebinding.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mutation {
    Assign,
    Bind,
    Unset,
}

impl From<Binding> for Mutation {
    fn from(binding: Binding) -> Self {
        match binding {
            Binding::ByValue => Self::Assign,
            Binding::ByReference => Self::Bind,
        }
    }
}
