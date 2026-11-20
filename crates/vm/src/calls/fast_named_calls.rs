//! Calls by name that the interpreter loop prepares without the VM.
//!
//! ```php
//! namespace App;
//! $name = trim($input);   // InitCall: App\trim if declared, else \trim
//! log_line($name);        // InitCall of a PHP function declared elsewhere
//! ```
//!
//! The first `InitCall` of a name goes through the VM, which finds the
//! function (with PHP's namespace fallback, at the instruction itself for an
//! unqualified name in a namespace) and remembers what it found for the
//! file, or for that instruction. A declared function cannot go away, so
//! from then on the loop prepares the call from that record: a window for a
//! PHP function, or a window and the callee's description for native code.
//! A name not linked yet, or a callee that is a method or closure, answers
//! [`Step::Slow`] without having changed anything.

use phera_bytecode::SymbolId;

use crate::calls::call_state::CallTarget;
use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::pending::{LinkedFunction, prepare_general};
use crate::calls::php_windows::push_php_call;
use crate::calls::prepared_call::CallKind;
use crate::unit_caches::CachedSite;

impl Machine<'_> {
    /// `InitCall` of a function the file has linked already.
    #[inline(never)]
    pub(super) fn prepare_named_call(&mut self, position: Position, symbol: SymbolId) -> Step {
        let Some(linked) = self.linked(position, symbol) else {
            return Step::Slow;
        };
        let parts = (&mut *self.stack, &mut *self.registers);
        match linked {
            LinkedFunction::Php(function) => {
                push_php_call(self.code, parts, function, CallKind::Function, []);
            }
            LinkedFunction::Other(callee)
                if matches!(
                    callee.target,
                    CallTarget::Native(_) | CallTarget::Intrinsic(_)
                ) =>
            {
                prepare_general(parts.0, parts.1, callee);
            }
            _ => return Step::Slow,
        }
        Step::Done(position.ip)
    }

    /// What the file, or for a namespace fallback this instruction, linked
    /// the name to.
    fn linked(&self, position: Position, symbol: SymbolId) -> Option<LinkedFunction> {
        let function = position.frame.function;
        let cache = self.code.cache(function);
        let linked = cache
            .symbols
            .try_borrow()
            .ok()?
            .get(symbol.index())?
            .clone()?;
        if !matches!(linked, LinkedFunction::NamespaceFallback) {
            return Some(linked);
        }
        // The VM keys the site by the position after the instruction,
        // which is where the loop stands now.
        let site = cache
            .site(self.code.facts(function), position.ip)?
            .try_borrow()
            .ok()?;
        match &*site {
            CachedSite::Function(linked) => Some((**linked).clone()),
            _ => None,
        }
    }
}
