//! A function's instructions, shared by the compiled file and by every
//! request that runs them.
//!
//! ```php
//! function add($a, $b) { return $a + $b; }
//! echo add(1, 2);
//! ```
//!
//! The compiler builds `add`'s instructions like a `Vec`. Once the file is
//! compiled they never change. Each request that loads the file keeps a
//! handle on them beside what it knows about `add` (the VM's
//! `FunctionFacts`), so that entering `add`, or returning to the code that
//! called it, finds the instructions in two steps rather than through the
//! file, its function table and the function. The handle is a reference
//! count, so loading a file copies no instructions.

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::Instruction;

/// A function's instructions; reads as a slice.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Code(Arc<Vec<Instruction>>);

impl Code {
    /// Appends an instruction. Only the compiler does this, while the
    /// function has no other owner, so nothing is copied.
    pub fn push(&mut self, instruction: Instruction) {
        Arc::make_mut(&mut self.0).push(instruction);
    }
}

impl Deref for Code {
    type Target = [Instruction];

    fn deref(&self) -> &[Instruction] {
        self.0.as_slice()
    }
}

/// The compiler's passes rewrite instructions in place, before the file is
/// shared; a write after that would copy the instructions first.
impl DerefMut for Code {
    fn deref_mut(&mut self) -> &mut [Instruction] {
        Arc::make_mut(&mut self.0).as_mut_slice()
    }
}

impl From<Vec<Instruction>> for Code {
    fn from(instructions: Vec<Instruction>) -> Self {
        Self(Arc::new(instructions))
    }
}

impl<'a> IntoIterator for &'a Code {
    type Item = &'a Instruction;
    type IntoIter = std::slice::Iter<'a, Instruction>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut Code {
    type Item = &'a mut Instruction;
    type IntoIter = std::slice::IterMut<'a, Instruction>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
