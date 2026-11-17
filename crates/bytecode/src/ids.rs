//! Typed indices. Each is a plain number, but a distinct type per table means
//! the compiler cannot pass a name index where a constant index is expected.

use std::fmt;

/// Defines an index type with a compact `Debug` form for the disassembler.
macro_rules! index {
    ($(#[$meta:meta])* $name:ident($inner:ty), $prefix:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub $inner);

        impl $name {
            pub fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }
    };
}

index!(
    /// A slot in the running function's frame. The first slots are the named
    /// local variables (parameters first); temporaries follow.
    Reg(u16), "r"
);
index!(
    /// An instruction index within the same function: where a jump lands.
    Target(u32), "@"
);
index!(
    /// An entry in [`crate::CompiledUnit::constants`].
    ConstId(u32), "const#"
);
index!(
    /// An entry in [`crate::CompiledUnit::names`].
    NameId(u32), "name#"
);
index!(
    /// An entry in [`crate::CompiledUnit::symbols`]: a function or constant
    /// referred to by name.
    SymbolId(u32), "symbol#"
);
index!(
    /// An entry in [`crate::CompiledUnit::types`].
    TypeId(u32), "type#"
);
index!(
    /// An entry in [`crate::CompiledUnit::functions`].
    FunctionId(u32), "function#"
);
index!(
    /// An entry in [`crate::Function::paths`].
    PathId(u32), "path#"
);
index!(
    /// An entry in [`crate::Function::statics`].
    StaticId(u16), "static#"
);
index!(
    /// One of the frame's `foreach` iterators. Nested loops use different
    /// ones; loops that follow each other reuse them.
    IterId(u16), "iter#"
);

index!(
    /// An entry in [`crate::CompiledUnit::classes`].
    ClassId(u32), "class#"
);
