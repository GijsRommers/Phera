//! What only some frames need, kept out of the frame itself.
//!
//! ```php
//! function plain($a) { return $a + 1; }          // never touches this table
//! function loop($xs) { foreach ($xs as $x) {} }  // iterator state lives here
//! function named() { $$n = 1; }                  // variables found by name too
//! ```
//!
//! The table is a `Vec` indexed by frame depth. Entry `d` belongs to whatever
//! frame is at depth `d`; a frame that stores something sets its "extras"
//! flag, and the entry is emptied when that frame returns or unwinds.

use crate::dynamic_variables::{DynamicVariables, VariableScope};
use crate::iterators::Iterators;

/// The optional state of one frame.
pub(crate) struct FrameExtras {
    /// The called class of a static method frame, when it is not the class
    /// that declares the method (`static::` after `Child::create()`).
    pub(crate) called: Option<crate::classes::ClassIndex>,
    /// Variables created by name (`$$name`, `extract`) after entry.
    pub(crate) dynamic: DynamicVariables,
    /// The register count once variables were created by name.
    pub(crate) dynamic_count: Option<usize>,
    /// Active `foreach` loops.
    pub(crate) iterators: Iterators,
    /// How the call passed its arguments, when that differs from "one per
    /// parameter".
    pub(crate) arguments: ArgumentFacts,
    /// PHP keeps the original list when the variadic local is changed or unset.
    pub(crate) variadic: Option<std::rc::Rc<phera_value::PhpArray>>,
    /// A property operation resumes only when this hook returns normally.
    pub(crate) property_hook: Option<Box<crate::property_hook_continuation::HookContinuation>>,
    /// This frame pushed a `PropertyGuard` for a synchronous `__get`/`__set`
    /// entry that must be popped when the frame returns or unwinds.
    pub(crate) magic_guard: bool,
}

/// How many arguments a call passed and where the surplus ones are, for
/// `func_get_args()`, traces and variadic parameters.
#[derive(Clone, Copy, Default)]
pub(crate) struct ArgumentFacts {
    /// Arguments passed, when not exactly one per parameter.
    pub(crate) supplied: Option<usize>,
    /// What `func_num_args()` reports, when not exactly one per parameter.
    pub(crate) count: Option<usize>,
    /// Arguments past the declared parameters, stored after the frame's registers.
    pub(crate) extra: usize,
    /// Where a copy of the original arguments starts, for a variadic
    /// function that calls `func_get_args()`.
    pub(crate) saved: Option<usize>,
}

impl ArgumentFacts {
    /// One argument per parameter, nothing saved.
    fn is_default(&self) -> bool {
        self.supplied.is_none() && self.count.is_none() && self.extra == 0 && self.saved.is_none()
    }
}

impl FrameExtras {
    /// An entry that holds nothing.
    pub(crate) fn new() -> Self {
        Self {
            called: None,
            dynamic: DynamicVariables::new(VariableScope::Static),
            dynamic_count: None,
            iterators: Iterators::new(0),
            arguments: ArgumentFacts::default(),
            variadic: None,
            property_hook: None,
            magic_guard: false,
        }
    }

    /// A fast return to a register can release the called class alone.
    /// Suspended hooks have no return register and never take that path.
    pub(crate) fn plain_return_can_release(&self) -> bool {
        self.dynamic_count.is_none()
            && self.dynamic.is_empty()
            && self.iterators.is_empty()
            && self.arguments.is_default()
            && self.variadic.is_none()
            && !self.magic_guard
    }

    /// Releases what the frame owned, keeping the storage for the next frame
    /// at this depth.
    pub(crate) fn clear(&mut self) {
        self.called = None;
        self.dynamic.reset(VariableScope::Static);
        self.dynamic_count = None;
        self.iterators.clear();
        self.arguments = ArgumentFacts::default();
        self.variadic = None;
        self.property_hook = None;
        self.magic_guard = false;
    }
}

impl crate::snapshot::ForkCopy for FrameExtras {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        if self.property_hook.is_some() || self.magic_guard {
            fork.refuse_state(
                "property hook",
                "Phera\\snapshot() was called inside a property hook",
            );
        }
        FrameExtras {
            called: self.called,
            dynamic: self.dynamic.clone(),
            dynamic_count: self.dynamic_count,
            iterators: self.iterators.fork_copy(fork),
            arguments: self.arguments,
            variadic: self.variadic.fork_copy(fork),
            property_hook: None,
            magic_guard: false,
        }
    }
}
