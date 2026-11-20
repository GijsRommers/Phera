//! Which instructions of a function have a site: a per-request cache that
//! the VM fills the first time the instruction runs.
//!
//! ```php
//! function name(User $user) {
//!     return $user->name;      // Fetch: the site remembers where `name` is stored
//! }
//! namespace App;
//! $parts = explode(',', $s);   // InitCall: the site remembers that `explode` is \explode
//! ```
//!
//! Instructions that reach a place through a path, and calls by a name that
//! a namespace may resolve differently at each call site, have one. Every
//! other instruction has none, and a request keeps a cache cell only for
//! the instructions that have a site: that is far fewer cells to make when
//! a request loads a file, and to copy when a request starts from a
//! snapshot. The table is worked out once per compiled function, the first
//! time a request asks, and shared by every request after that.

use std::sync::OnceLock;

use crate::Instruction;

/// Marks a position that has no site.
const NONE: u32 = u32::MAX;

/// The site number of each position after an instruction (the VM keys a
/// site by where execution continues), or none.
#[derive(Debug, Clone, Default)]
pub struct SiteTable(OnceLock<Box<[u32]>>);

impl SiteTable {
    /// The site of the instruction that ends at `position`, if it has one.
    #[inline]
    pub fn site(&self, instructions: &[Instruction], position: usize) -> Option<usize> {
        let slot = *self.table(instructions).get(position)?;
        (slot != NONE).then_some(slot as usize)
    }

    /// How many sites the function has.
    pub fn count(&self, instructions: &[Instruction]) -> usize {
        self.table(instructions)
            .iter()
            .filter(|slot| **slot != NONE)
            .count()
    }

    #[inline]
    fn table(&self, instructions: &[Instruction]) -> &[u32] {
        match self.0.get() {
            Some(table) => table,
            None => self.fill(instructions),
        }
    }

    /// Works the table out, the first time a request asks.
    #[cold]
    #[inline(never)]
    fn fill(&self, instructions: &[Instruction]) -> &[u32] {
        self.0.get_or_init(|| {
            let mut next = 0;
            // One more position than instructions: a site is keyed by the
            // position after its instruction.
            std::iter::once(NONE)
                .chain(instructions.iter().map(|instruction| {
                    if has_site(instruction) {
                        next += 1;
                        next - 1
                    } else {
                        NONE
                    }
                }))
                .collect()
        })
    }
}

/// The table is derived from the instructions, so it never makes two
/// functions different.
impl PartialEq for SiteTable {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// Whether an instruction looks up a site.
fn has_site(instruction: &Instruction) -> bool {
    matches!(
        instruction,
        Instruction::Fetch { .. }
            | Instruction::FetchIndex { .. }
            | Instruction::AssignPath { .. }
            | Instruction::AssignPathFromLocal { .. }
            | Instruction::AssignOpPath { .. }
            | Instruction::IncDecPath { .. }
            | Instruction::UnsetPath { .. }
            | Instruction::IssetPath { .. }
            | Instruction::SendPath { .. }
            | Instruction::MakeReference { .. }
            | Instruction::BindReference { .. }
            | Instruction::IterStartReference { .. }
            | Instruction::InitCall { .. }
    )
}
