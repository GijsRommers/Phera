//! Which function an unqualified built-in call in a namespace means.
//!
//! ```php
//! namespace App;
//! function check($x) { return is_null($x); }
//! check(1);                                   // the global is_null
//! function is_null($x) { return 'mine'; }     // declared later
//! check(1);                                   // still the global is_null
//! function fresh($x) { return is_null($x); }
//! fresh(1);                                   // 'mine': a site that had not run
//! ```
//!
//! `GuardBuiltin` stands before such a call (see the compiler's
//! `namespaced_builtins`). The first time a site runs, PHP's rule decides:
//! `App\is_null` when a function of that name is declared then, the global
//! function otherwise. The answer is kept in the instruction's site cell, as
//! PHP keeps the first target a call site found, so every later run is one
//! read of that cell, by the instruction's position, and never a name lookup.
//! On the global answer the built-in's own instruction runs next; otherwise
//! the guard jumps to the general call that follows it.

use phera_bytecode::{SymbolId, Target};
use phera_extension_api::NativeError;

use crate::Vm;
use crate::unit_caches::CachedSite;

/// What a guarded site found when it first ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Resolution {
    /// No function of the namespace: the global built-in.
    Builtin,
    /// The namespace declares the function: the general call.
    Namespaced,
}

impl Vm {
    /// `GuardBuiltin`: fall through to the built-in, or jump to `call`.
    pub(crate) fn execute_guard_builtin(
        &mut self,
        function: SymbolId,
        call: Target,
    ) -> Result<(), NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("builtin guard frame"))?;
        let site = frame
            .site_cache()
            .ok_or_else(|| internal_error!("builtin guard site"))?;
        let found = match &*site.borrow() {
            CachedSite::BuiltinGuard(resolution) => Some(*resolution),
            _ => None,
        };
        let resolution = match found {
            Some(resolution) => resolution,
            None => {
                let name = &frame
                    .unit()
                    .symbols
                    .get(function.index())
                    .ok_or_else(|| internal_error!("builtin guard symbol"))?
                    .name;
                let resolution = if self.exists(name) {
                    Resolution::Namespaced
                } else {
                    Resolution::Builtin
                };
                *site.borrow_mut() = CachedSite::BuiltinGuard(resolution);
                resolution
            }
        };
        if resolution == Resolution::Namespaced {
            self.jump(call)?;
        }
        Ok(())
    }
}
