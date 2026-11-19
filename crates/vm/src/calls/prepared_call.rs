//! A call whose arguments are being evaluated: a few numbers on a stack.
//!
//! ```php
//! $total = add($x, scale($y, 2));
//! ```
//!
//! `add` is prepared first. Its record says which function it calls and
//! where that function's window of registers starts. While the arguments are
//! evaluated, `scale` is prepared above it, receives `$y` and `2`, runs and
//! returns; its result is sent to `add`, and then `add` runs. A record is a
//! few numbers copied onto and off a `Vec` that keeps its capacity, so
//! preparing a call allocates nothing and touches no reference count.
//!
//! What only some calls need is kept in a [`CallDetails`] entry beside the
//! record: the full description of a callee that is not plain PHP code (a
//! native function, an intrinsic, a method), and the state of named
//! arguments. Everything else, such as a parameter's name for a named
//! argument, is read from the callee's bytecode when it is needed.

use phera_bytecode::{CompiledUnit, Function, Type};
use phera_value::PhpString;

use crate::calls::call_state::{Callee, ParameterSpec};
use crate::code_table::FunctionIndex;

/// One call being prepared.
#[derive(Clone, Copy)]
pub(crate) struct PreparedCall {
    pub(crate) callee: PreparedCallee,
    pub(crate) kind: CallKind,
    /// The called class (`static::`) of a static method call, when it is not
    /// the class that declares the method. An instance method's called class
    /// is the class of `$this`.
    pub(crate) called: Option<crate::classes::ClassIndex>,
    /// Where the callee's first parameter goes in the register stack.
    base: u32,
    /// One past the highest argument position filled so far.
    pub(crate) sent: u32,
    /// What `func_num_args()` will report: named arguments that a variadic
    /// parameter collects do not count.
    pub(crate) counted: u32,
    pub(crate) flags: CallFlags,
}

/// What a prepared call will run.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreparedCallee {
    /// A function written in PHP, entered with nothing but its arguments.
    Php(FunctionIndex),
    /// `new` of a class without a constructor: the arguments are evaluated
    /// and dropped, and the call gives the new object, kept below them.
    Constructed,
    /// Anything else; its [`Callee`] is in the call's details.
    General,
}

/// What kind of code a call runs, which says what its window holds below
/// the first parameter.
///
/// ```text
/// function:     [ $a, $b, locals... ]
/// method:       [ $this | $a, $b, locals... ]       (empty for a static method)
/// closure:      [ closure, $this | $a, locals... ]  ($this empty when unbound)
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CallKind {
    Function = 0,
    Method = 1,
    /// A constructor called by `new`: the call evaluates to `$this`.
    Constructor = 2,
    Closure = 3,
}

impl CallKind {
    /// How many registers the window holds below the first parameter.
    #[inline]
    pub(crate) fn prefix(self) -> usize {
        match self {
            Self::Function => 0,
            Self::Method | Self::Constructor => 1,
            Self::Closure => 2,
        }
    }
}

/// Small facts about a prepared call, one bit each.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CallFlags(u8);

impl CallFlags {
    /// An argument did not pass its type's quick test; it is checked (and
    /// perhaps converted) when the call starts, after every argument is in.
    pub(crate) const DEFERRED_CHECKS: Self = Self(1);
    /// A named argument was sent, so a positional one is now an error.
    pub(crate) const NAMED: Self = Self(1 << 1);
    /// The call has an entry on the details stack.
    pub(crate) const DETAILS: Self = Self(1 << 2);
    /// The call is an explicit `Closure::__invoke`: it binds its arguments
    /// as an internal function does (weakly typed, without the caller's
    /// location in errors), and messages name it `Closure::__invoke`.
    pub(crate) const INVOKE: Self = Self(1 << 3);
    /// A deferred PHP argument can need an interpreter call to __toString.
    pub(crate) const STRING_ARGUMENT: Self = Self(1 << 4);
    /// The general callee is a trampoline for __call or __callStatic.
    pub(crate) const MAGIC: Self = Self(1 << 5);
    /// A native window accepts one positional mixed argument by value.
    pub(crate) const WINDOW_ARGUMENT: Self = Self(1 << 6);

    pub(crate) fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 != 0
    }

    pub(crate) fn insert(&mut self, flag: Self) {
        self.0 |= flag.0;
    }

    pub(crate) fn remove(&mut self, flag: Self) {
        self.0 &= !flag.0;
    }
}

impl PreparedCall {
    /// A call to `callee` whose first parameter goes to register `base`.
    pub(crate) fn new(callee: PreparedCallee, base: usize) -> Self {
        Self::of_kind(callee, CallKind::Function, base)
    }

    /// A call of `kind` whose window starts at register `start`: its first
    /// parameter goes after what the kind keeps below it.
    #[inline]
    pub(crate) fn of_kind(callee: PreparedCallee, kind: CallKind, start: usize) -> Self {
        Self {
            callee,
            kind,
            called: None,
            base: (start + kind.prefix()) as u32,
            sent: 0,
            counted: 0,
            flags: CallFlags::default(),
        }
    }

    /// Where the callee's first parameter goes in the register stack.
    #[inline]
    pub(crate) fn base(&self) -> usize {
        self.base as usize
    }

    /// Where the call's window starts: `$this` or the closure object, if the
    /// kind keeps one, else the first parameter.
    #[inline]
    pub(crate) fn start(&self) -> usize {
        self.base() - self.kind.prefix()
    }

    /// A suspended caller moved this prepared argument window with its own.
    pub(crate) fn moved_to(mut self, base: usize) -> Self {
        self.base = base as u32;
        self
    }

    /// The register stack moved up by one below this call's window (a
    /// variable was created by name in the frame that prepares it).
    pub(crate) fn shift_up(&mut self) {
        self.base += 1;
    }

    /// Whether every argument arrived by position and passed its quick test,
    /// so that entering the callee is only pushing a frame.
    #[inline]
    pub(crate) fn is_plain(&self) -> bool {
        !self.flags.contains(CallFlags::DEFERRED_CHECKS) && !self.flags.contains(CallFlags::NAMED)
    }

    /// Record that the argument at position `index` was filled; `counted`
    /// is false for a named argument that a variadic parameter collects.
    #[inline]
    pub(crate) fn mark_sent(&mut self, index: usize, counted: bool) {
        let next = index as u32 + 1;
        self.sent = self.sent.max(next);
        if counted {
            self.counted = self.counted.max(next);
        }
    }
}

/// What only some prepared calls need, kept beside the record.
#[derive(Default)]
pub(crate) struct CallDetails {
    /// The callee, when it is not plain PHP code.
    pub(crate) callee: Option<Callee>,
    /// The parameter name the next `Send` is for.
    pub(crate) named: Option<PhpString>,
    /// Named arguments that the variadic parameter collects, by position.
    pub(crate) extra_named: Vec<(usize, PhpString)>,
}

/// The parameters of a prepared call's callee, read from its bytecode or
/// from its description.
#[derive(Clone, Copy)]
pub(crate) enum Parameters<'a> {
    Php {
        function: &'a Function,
        unit: &'a CompiledUnit,
    },
    Listed(&'a [ParameterSpec]),
}

impl<'a> Parameters<'a> {
    /// How many parameters are declared.
    pub(crate) fn len(self) -> usize {
        match self {
            Self::Php { function, .. } => function.parameters.len(),
            Self::Listed(parameters) => parameters.len(),
        }
    }

    pub(crate) fn name(self, index: usize) -> &'a str {
        match self {
            Self::Php { function, unit } => &unit.names[function.parameters[index].name.index()],
            Self::Listed(parameters) => &parameters[index].name,
        }
    }

    pub(crate) fn by_reference(self, index: usize) -> bool {
        match self {
            Self::Php { function, .. } => function.parameters[index].by_reference,
            Self::Listed(parameters) => parameters[index].by_reference,
        }
    }

    pub(crate) fn is_variadic(self, index: usize) -> bool {
        match self {
            Self::Php { function, .. } => function.parameters[index].variadic,
            Self::Listed(parameters) => parameters[index].variadic,
        }
    }

    /// The declared type of parameter `index`.
    pub(crate) fn ty(self, index: usize) -> Option<&'a Type> {
        match self {
            Self::Php { function, unit } => function.parameters[index]
                .ty
                .map(|ty| &unit.types[ty.index()]),
            Self::Listed(parameters) => parameters[index].ty.as_ref(),
        }
    }

    /// The parameter that receives argument `index`: its own, or for a
    /// surplus argument the variadic one, if there is one.
    pub(crate) fn receiving(self, index: usize) -> Option<usize> {
        if index < self.len() {
            return Some(index);
        }
        let last = self.len().checked_sub(1)?;
        self.is_variadic(last).then_some(last)
    }

    /// The position of the variadic parameter, if there is one.
    pub(crate) fn variadic(self) -> Option<usize> {
        (0..self.len()).find(|&index| self.is_variadic(index))
    }

    /// The position of the parameter called `name`, not counting the
    /// variadic one.
    pub(crate) fn position_of(self, name: &[u8]) -> Option<usize> {
        (0..self.len())
            .find(|&index| !self.is_variadic(index) && self.name(index).as_bytes() == name)
    }
}
