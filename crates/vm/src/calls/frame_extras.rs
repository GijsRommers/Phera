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

use std::rc::Rc;

use phera_value::Object;

use crate::dynamic_variables::{DynamicVariables, VariableScope};
use crate::iterators::Iterators;

/// The optional state of one frame.
pub(crate) struct FrameExtras {
    /// The closure object being run; it owns the closure's captures.
    pub(crate) closure: Option<Rc<Object>>,
    /// `$this`, the class scope and the called class of a method frame.
    pub(crate) method: Option<crate::classes::MethodContext>,
    /// Variables created by name (`$$name`, `extract`) after entry.
    pub(crate) dynamic: DynamicVariables,
    /// The register count once variables were created by name.
    pub(crate) dynamic_count: Option<usize>,
    /// Active `foreach` loops.
    pub(crate) iterators: Iterators,
    /// How the call passed its arguments, when that differs from "one per
    /// parameter".
    pub(crate) arguments: ArgumentFacts,
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

impl FrameExtras {
    /// An entry that holds nothing.
    pub(crate) fn new() -> Self {
        Self {
            closure: None,
            method: None,
            dynamic: DynamicVariables::new(VariableScope::Static),
            dynamic_count: None,
            iterators: Iterators::new(0),
            arguments: ArgumentFacts::default(),
        }
    }

    /// Releases what the frame owned, keeping the storage for the next frame
    /// at this depth.
    pub(crate) fn clear(&mut self) {
        self.closure = None;
        self.method = None;
        self.dynamic.reset(VariableScope::Static);
        self.dynamic_count = None;
        self.iterators.clear();
        self.arguments = ArgumentFacts::default();
    }
}
