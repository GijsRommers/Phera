//! Literal-array membership and pure synchronous built-ins executed inside the
//! interpreter loop.
//!
//! ```php
//! if (in_array($role, ['admin', 'editor'], true)) { ... }
//! ```

use phera_bytecode::{ConstId, Reg};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::fast::{ScalarStep, store};

impl Machine<'_> {
    /// `InLiteralArray`: `in_array($needle, ['a', 'b', ...], $strict)` in the loop.
    #[inline(never)]
    pub(super) fn in_literal_array<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        (needle, haystack, strict): (Reg, ConstId, bool),
    ) -> Step {
        let base = position.frame.base();
        let found = {
            let Some(Value::Array(values)) = self
                .code
                .cache(position.frame.function)
                .constants
                .get(haystack.index())
            else {
                return Step::Slow;
            };
            let [Some(needle_slot)] = self.registers.window(base + needle.index(), 1) else {
                return Step::Slow;
            };
            let needle_val = needle_slot.deref();
            match &*needle_val {
                Value::InternedStr(needle) => values.values().any(|value| match value {
                    Value::InternedStr(other) => {
                        std::ptr::eq(*other, *needle) || other.as_bytes() == needle.as_bytes()
                    }
                    Value::String(other) => other.as_bytes() == needle.as_bytes(),
                    _ => false,
                }),
                Value::String(needle) => values.values().any(|value| {
                    value
                        .as_str_bytes()
                        .is_some_and(|other| other == needle.as_bytes())
                }),
                Value::Int(needle) if strict => values
                    .values()
                    .any(|value| matches!(value, Value::Int(value) if value == needle)),
                _ if strict => false,
                _ => return Step::Slow,
            }
        };
        let slots = self.registers.tail_mut(base);
        match store::<GUARDED, _>(slots, destination, Value::Bool(found)) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }
}
