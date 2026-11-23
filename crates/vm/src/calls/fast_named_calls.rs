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

use phera_extension_api::{NativeFn, NativeId};

use crate::calls::call_state::{CallTarget, Callee};
use crate::calls::callback_binding::ArgumentBinding;
use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::pending::{LinkedFunction, prepare_general};
use crate::calls::php_windows::push_php_call;
use crate::calls::prepared_call::CallKind;
use crate::calls::prepared_call::{PreparedCall, PreparedCallee};
use crate::code_table::FunctionIndex;
use crate::unit_caches::CachedSite;

/// What the loop does for an `InitCall`, decided while the file's record
/// is borrowed, so that nothing of it is cloned but a general callee.
enum NamedCall {
    Php(FunctionIndex),
    /// A synchronous native function: its number and how many registers
    /// its arguments take.
    Native(NativeId, usize),
    General(Callee),
}

impl Machine<'_> {
    /// `InitCall` of a function the file has linked already.
    #[inline(never)]
    pub(super) fn prepare_named_call(&mut self, position: Position, symbol: SymbolId) -> Step {
        let Some(call) = self.linked(position, symbol, named_call).flatten() else {
            return Step::Slow;
        };
        let parts = (&mut *self.stack, &mut *self.registers);
        match call {
            NamedCall::Php(function) => {
                push_php_call(self.code, parts, function, CallKind::Function, []);
            }
            // A lean record (see `calls::native_calls`).
            NamedCall::Native(id, slots) => {
                let (stack, registers) = parts;
                let base = registers.len();
                registers.grow_to(base + slots);
                stack.push_call(PreparedCall::new(PreparedCallee::Native(id), base), None);
            }
            NamedCall::General(callee) => prepare_general(parts.0, parts.1, callee),
        }
        Step::Done(position.ip)
    }

    /// `decide` applied to what the file, or for a namespace fallback this
    /// instruction, linked the name to.
    fn linked<R>(
        &self,
        position: Position,
        symbol: SymbolId,
        decide: impl FnOnce(&LinkedFunction) -> R,
    ) -> Option<R> {
        let function = position.frame.function;
        let cache = self.code.cache(function);
        let symbols = cache.symbols.try_borrow().ok()?;
        let linked = symbols.get(symbol.index())?.as_ref()?;
        if !matches!(linked, LinkedFunction::NamespaceFallback) {
            return Some(decide(linked));
        }
        // The VM keys the site by the position after the instruction,
        // which is where the loop stands now.
        self.code
            .read_site(function, position.ip, |site| match site {
                CachedSite::Function(linked) => Some(decide(linked)),
                _ => None,
            })
            .flatten()
    }
}

/// How the loop prepares a call of `linked`, if it does: a PHP function, a
/// synchronous native function linked from a name, or other native code
/// (the callee's description). Methods and closures go to the VM.
fn named_call(linked: &LinkedFunction) -> Option<NamedCall> {
    match linked {
        LinkedFunction::Php(function) => Some(NamedCall::Php(*function)),
        LinkedFunction::Other(Callee {
            argument_binding: ArgumentBinding::Caller,
            target: CallTarget::Native(entry),
            ..
        }) if matches!(entry.handler, NativeFn::Sync(_)) && entry.id != NativeId::NONE => {
            Some(NamedCall::Native(entry.id, entry.params.len().max(1)))
        }
        LinkedFunction::Other(callee)
            if matches!(
                callee.target,
                CallTarget::Native(_) | CallTarget::Intrinsic(_)
            ) =>
        {
            Some(NamedCall::General(callee.clone()))
        }
        _ => None,
    }
}
