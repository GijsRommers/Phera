//! A call whose result nobody uses frees that result at the `return`.
//!
//! ```php
//! class D { function __construct(public $n) {} function __destruct() { echo "~$this->n\n"; } }
//! function f() { $local = new D('local'); return new D('result'); }
//! f();   // ~result, then ~local
//! ```
//!
//! PHP does not even store the result of a call that is used as a
//! statement: `return` frees the returned temporary at once, before the
//! function's variables are released. A returned variable (`return
//! $local;`) is instead freed with the other variables, in their order.
//!
//! Phera's compiler stores every result and clears it right after the call
//! when it is unused. So a return looks at the caller's next instructions:
//! if they are clears that include the result's register, the result is
//! unused, and a temporary result is dropped before the frame's variables.
//! Only a value that can own an object is worth the look.

use phera_bytecode::{Instruction, Reg};
use phera_value::Value;

/// Whether the caller's instructions from `ip` on clear `destination`
/// before doing anything else.
pub(crate) fn result_discarded(caller: &[Instruction], ip: usize, destination: Reg) -> bool {
    caller
        .iter()
        .skip(ip)
        .map_while(|instruction| match instruction {
            Instruction::Clear { reg } => Some(*reg),
            _ => None,
        })
        .any(|register| register == destination)
}

/// Whether freeing `value` could run a destructor.
#[inline]
pub(crate) fn may_own_objects(value: &Value) -> bool {
    matches!(
        value,
        Value::Array(_) | Value::Object(_) | Value::Reference(_)
    )
}
