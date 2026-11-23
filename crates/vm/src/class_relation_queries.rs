//! The relation queries `is_a` / `is_subclass_of`, answered in the interpreter
//! loop.
//!
//! ```php
//! is_a($child, 'Marker');      // the object's class, its parents and interfaces
//! is_subclass_of($child, 'Base');  // the same, except the class itself
//! is_a('Child', 'Base', true); // a string first argument only with allow_string
//! ```
//!
//! The general handler ([`Vm::class_intrinsic`]) is the full behaviour: it
//! coerces arguments, runs the autoloader for a string first argument when
//! `allow_string` allows it, and raises PHP's errors. A plain positional call
//! (an object or string first, a string target, an optional bool flag) needs
//! no PHP code, so the loop answers it from the call's argument window, like
//! the queries in `class_queries.rs`. Every other shape returns
//! `BatchExit::NativeCall` with the window intact, for the general handler.

use phera_bytecode::Reg;
use phera_value::Value;

use crate::Vm;
use crate::calls::execution_loop::BatchExit;
use crate::calls::prepared_call::PreparedCall;
use crate::class_queries::ResolvedName;

/// Which relation `is_a` / `is_subclass_of` asks for, made by the routing
/// arm in `calls::sync_native`.
#[derive(Clone, Copy)]
pub(crate) enum RelationQuery {
    /// `is_a`: the class itself, a parent, or an implemented interface.
    IsA,
    /// `is_subclass_of`: the same, except the class itself.
    IsSubclassOf,
}

impl RelationQuery {
    /// PHP's default for the third argument: `is_a` does not look a string
    /// first argument up, `is_subclass_of` does.
    fn default_allow_string(self) -> bool {
        matches!(self, Self::IsSubclassOf)
    }
}

impl Vm {
    /// `is_a` / `is_subclass_of` for a plain call: the answer from the class
    /// table, or the exit for the general handler.
    #[inline(never)]
    pub(crate) fn relation_query_call(
        &mut self,
        query: RelationQuery,
        call: PreparedCall,
        destination: Reg,
    ) -> Option<BatchExit> {
        let answer = self
            .plain_window(call, &[2, 3])
            .and_then(|window| match window {
                [Some(first), Some(target)] => {
                    self.relation_answer(query, first, target, query.default_allow_string())
                }
                [Some(first), Some(target), Some(Value::Bool(allow_string))] => {
                    self.relation_answer(query, first, target, *allow_string)
                }
                _ => None,
            });
        self.finish_query(answer.map(Value::Bool), call, destination)
    }

    /// The answer for an `is_a` / `is_subclass_of` call whose arguments are
    /// `first` (an object or a class name), `target` (a class name) and
    /// `allow_string`.
    fn relation_answer(
        &self,
        query: RelationQuery,
        first: &Value,
        target: &Value,
        allow_string: bool,
    ) -> Option<bool> {
        // Only a valid UTF-8 target takes this path, for the same reason as
        // in `exists_answer`.
        let target = std::str::from_utf8(target.as_string()?.as_bytes()).ok()?;
        let class = match first {
            // An object's class is known; PHP never autoloads it.
            Value::Object(object) => self.classes.for_object(object),
            name => {
                let name = std::str::from_utf8(name.as_string()?.as_bytes()).ok()?;
                if !allow_string {
                    // Without allow_string a string first argument names no
                    // class, even one that is loaded.
                    None
                } else {
                    match self.resolve_name(name, true) {
                        ResolvedName::Found(class) => Some(class),
                        ResolvedName::Absent => None,
                        ResolvedName::Fallback => return None,
                    }
                }
            }
        };
        // The second argument is only looked up in the table; PHP never
        // autoloads it.
        let target_class = self.classes.get(target);
        class.zip(target_class).and_then(|(class, target)| {
            // A walk that outgrew its bounds (`None`) falls back to the
            // general handler.
            class.derives_from(&target).map(|derives| {
                derives
                    && !(matches!(query, RelationQuery::IsSubclassOf)
                        && class.index == target.index)
            })
        })
    }
}
