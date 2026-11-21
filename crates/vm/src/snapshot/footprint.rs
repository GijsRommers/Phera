//! What one fork allocates, by structure, in debug builds:
//!
//! ```sh
//! PHERA_FORK_FOOTPRINT=1 target/debug/phera serve public
//! # fork footprint: 1,234,567 bytes
//! #   site cells                                   347,136
//! #   per-symbol resolution slots                  816,128
//! #   ...
//! ```
//!
//! The count is exact for what it names (heap bytes by capacity, plus each
//! `Rc`'s two counters) and leaves out what a fork shares instead of
//! copying: compiled code, strings, the literal tables, linked classes. It
//! is the number that steers the work towards a fork that copies only what
//! it changes; resident memory, which the allocator rounds and keeps, is
//! too noisy for that.

use phera_value::Fork;

use crate::calls::registers::Registers;
use crate::classes::class_state::ClassStates;
use crate::code_table::CodeTable;

pub(super) fn report(
    fork: &Fork<'_>,
    code: &CodeTable,
    registers: &Registers,
    statics: &ClassStates,
) {
    let mut parts = code.fork_footprint();
    parts.extend(fork.footprint());
    parts.push(("register file", registers.heap_bytes()));
    parts.push(("statics and evaluated constants", statics.heap_bytes()));
    let total: usize = parts.iter().map(|(_, bytes)| bytes).sum();
    let mut lines = format!("fork footprint: {total} bytes\n");
    for (name, bytes) in parts {
        lines.push_str(&format!("  {name:<44} {bytes:>10}\n"));
    }
    eprint!("{lines}");
}
