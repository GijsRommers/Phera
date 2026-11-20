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
//! Only a value that this is the last handle on is worth the look.

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

/// Whether dropping `value` could free an object (and so run a
/// destructor): it is the last handle on an array, object or reference.
#[inline]
pub(crate) fn frees_on_drop(value: &Value) -> bool {
    match value {
        Value::Array(array) => std::rc::Rc::strong_count(array) == 1,
        Value::Object(object) => std::rc::Rc::strong_count(object) == 1,
        Value::Reference(cell) => std::rc::Rc::strong_count(cell) == 1,
        _ => false,
    }
}
