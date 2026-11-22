//! `GuardBuiltin` in the interpreter loop.
//!
//! ```php
//! namespace App;
//! if (is_null($value)) { ... }   // GuardBuiltin, then BuiltinUnary
//! ```
//!
//! Once the site has run and found the global built-in (see
//! `crate::builtin_guards`), the loop steps past the guard to the built-in's
//! own instruction. A site that has not run yet, or that found a function of
//! the namespace, goes to the VM ([`Step::Slow`]).

use crate::builtin_guards::Resolution;
use crate::calls::execution_loop::{Machine, Position, Step};
use crate::unit_caches::CachedSite;

impl Machine<'_> {
    /// `GuardBuiltin`, when the site found the global built-in.
    pub(super) fn builtin_guard(&self, position: Position) -> Step {
        let function = position.frame.function;
        // The VM keys the site by the position after the instruction,
        // which is where the loop stands now.
        let builtin = self
            .code
            .cache(function)
            .site(self.code.facts(function), position.ip)
            .and_then(|site| site.try_borrow().ok())
            .is_some_and(|site| matches!(*site, CachedSite::BuiltinGuard(Resolution::Builtin)));
        if builtin {
            Step::Done(position.ip)
        } else {
            Step::Slow
        }
    }
}
