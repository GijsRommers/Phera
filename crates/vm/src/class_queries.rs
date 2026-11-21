//! `class_exists('A')` and its relatives, answered in the interpreter loop.
//!
//! ```php
//! class_exists('A');           // true when A is loaded; a miss may autoload it
//! class_exists('A', false);    // only asks the class table
//! interface_exists('Marker');  // the table says what kind of entry a name is
//! trait_exists('T'); enum_exists('S');
//! ```
//!
//! The general handler ([`Vm::class_intrinsic`]) is the full behaviour: it
//! coerces arguments, runs the autoloader for a miss when the call allows it,
//! and raises PHP's errors. A plain positional call whose first argument is
//! already a string and whose optional flag is already a bool needs no PHP
//! code, so the loop answers it from the call's argument window, the way
//! `spl_object_id` is answered: the name is looked up in the class table
//! (which strips a leading backslash and lowercases into a stack buffer,
//! without allocating), and the answer is the entry's kind. Every other
//! shape — a named argument, an argument still waiting for its coercion, a
//! name that is not valid UTF-8, too few or too many arguments — returns
//! `BatchExit::NativeCall` with the window intact, for the general handler.

use phera_bytecode::{ClassKind, Reg};
use phera_value::{PhpString, Value};

use crate::Vm;
use crate::calls::execution_loop::BatchExit;
use crate::calls::prepared_call::PreparedCall;
use crate::class_intrinsics::ClassFunction;

impl Vm {
    /// `class_exists`, `interface_exists`, `trait_exists` and `enum_exists`
    /// for a plain call: the answer from the class table, or `None` when the
    /// call has a shape the general handler must see.
    #[inline(never)]
    pub(crate) fn class_query_call(
        &mut self,
        function: ClassFunction,
        call: PreparedCall,
        destination: Reg,
    ) -> Option<BatchExit> {
        let count = usize::try_from(call.sent).unwrap_or(usize::MAX);
        let plain = call.is_plain()
            && (count == 1 || count == 2)
            && self
                .call_stack
                .top_details()
                .is_some_and(|details| details.named.is_none() && details.extra_named.is_empty());
        let answer = if plain {
            match self.registers.window(call.base(), count) {
                [Some(Value::String(name))] => self.exists_answer(function, name, true),
                [Some(Value::String(name)), Some(Value::Bool(autoload))] => {
                    self.exists_answer(function, name, *autoload)
                }
                _ => None,
            }
        } else {
            None
        };
        let Some(answer) = answer else {
            return Some(BatchExit::NativeCall(destination));
        };
        if self.call_stack.pop_call().is_err() {
            return Some(BatchExit::NativeCall(destination));
        }
        self.registers.truncate(call.start());
        self.set(destination, Value::Bool(answer));
        None
    }

    /// The answer for a `*_exists` call whose first argument is `name`:
    /// `Some(answer)` when no PHP code can change it, `None` to fall back to
    /// the general handler.
    fn exists_answer(
        &self,
        function: ClassFunction,
        name: &PhpString,
        autoload: bool,
    ) -> Option<bool> {
        // The general handler lossy-converts bytes that are not UTF-8 before
        // looking the name up; only a valid name takes this path.
        let name = std::str::from_utf8(name.as_bytes()).ok()?;
        if let Some(class) = self.classes.get(name) {
            return Some(match function {
                ClassFunction::ClassExists => class.kind.is_class(),
                ClassFunction::InterfaceExists => class.kind == ClassKind::Interface,
                ClassFunction::TraitExists => class.kind == ClassKind::Trait,
                ClassFunction::EnumExists => class.kind == ClassKind::Enum,
                _ => return None,
            });
        }
        if !autoload {
            return Some(false);
        }
        // A miss with autoloading allowed goes to the general handler, which
        // runs the registered loaders or loads a bundled `Phera\...` class.
        // With no loader registered and a name no bundled source can have,
        // the general lookup also answers "no class" without running PHP.
        if self.has_autoloaders() || may_load_bundled(name) {
            return None;
        }
        Some(false)
    }
}

/// Whether `name` could name a bundled class: the general lookup loads those
/// from their source even when no autoloader is registered.
fn may_load_bundled(name: &str) -> bool {
    let name = name.strip_prefix('\\').unwrap_or(name);
    let bytes = name.as_bytes();
    bytes.len() >= 6 && bytes[..6].eq_ignore_ascii_case(b"phera") && bytes[6] == b'\\'
}
