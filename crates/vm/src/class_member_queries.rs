//! The member queries `method_exists` / `property_exists`, answered in the
//! interpreter loop.
//!
//! ```php
//! method_exists($base, 'pubMethod');   // the class's method table
//! property_exists($base, 'pub');       // declared, static and own properties
//! ```
//!
//! The general handler ([`Vm::class_intrinsic`]) is the full behaviour: it
//! coerces arguments, runs the autoloader for a class name it does not have,
//! runs a lazy object's initializer or a native class's property reader for
//! `property_exists`, and raises PHP's errors. A plain positional call (an
//! object or class name, a string member name) needs none of that, so the
//! loop answers it from the call's argument window, like the class queries in
//! `class_queries.rs`. Every other shape returns `BatchExit::NativeCall` with
//! the window intact, for the general handler.

use phera_bytecode::{Reg, Visibility};
use phera_value::Value;

use crate::Vm;
use crate::calls::execution_loop::BatchExit;
use crate::calls::prepared_call::PreparedCall;
use crate::class_queries::ResolvedName;

/// Which member `method_exists` / `property_exists` asks for, made by the
/// routing arm in `calls::sync_native`.
#[derive(Clone, Copy)]
pub(crate) enum MemberQuery {
    Method,
    Property,
}

impl Vm {
    /// `method_exists` / `property_exists` for a plain call: the answer from
    /// the class, or the exit for the general handler.
    #[inline(never)]
    pub(crate) fn member_query_call(
        &mut self,
        query: MemberQuery,
        call: PreparedCall,
        destination: Reg,
    ) -> Option<BatchExit> {
        let answer = self
            .plain_window(call, &[2])
            .and_then(|window| match window {
                [Some(first), Some(name)] => self.member_answer(query, first, name),
                _ => None,
            });
        self.finish_query(answer.map(Value::Bool), call, destination)
    }

    /// The answer for a `method_exists` / `property_exists` call whose
    /// arguments are `first` (an object or a class name) and `name`.
    fn member_answer(&self, query: MemberQuery, first: &Value, name: &Value) -> Option<bool> {
        // Only a valid UTF-8 name takes this path, for the same reason as in
        // `class_queries`.
        let name = std::str::from_utf8(name.as_string()?.as_bytes()).ok()?;
        let class = match first {
            Value::Object(object) => {
                // The general handler runs PHP code for these two shapes of
                // `property_exists`: a lazy object may need its initializer,
                // and a native class answers for properties in its own state.
                if matches!(query, MemberQuery::Property)
                    && (object.lazy().is_some()
                        || crate::properties::native_property_access(object).is_some())
                {
                    return None;
                }
                self.classes.for_object(object)
            }
            class_name => {
                let class_name = std::str::from_utf8(class_name.as_string()?.as_bytes()).ok()?;
                match self.resolve_name(class_name, true) {
                    ResolvedName::Found(class) => Some(class),
                    ResolvedName::Absent => None,
                    ResolvedName::Fallback => return None,
                }
            }
        };
        let Some(class) = class else {
            return Some(false);
        };
        Some(match query {
            MemberQuery::Property => {
                class.name != phera_extension_api::serialization::INCOMPLETE_CLASS
                    && self.class_property_exists(first, &class, name)
            }
            MemberQuery::Method => {
                // The method table is keyed by lowercase names; a name that
                // does not fit the buffer falls back to the general handler.
                let mut buffer = [0u8; 64];
                let lower = lowercase64(name, &mut buffer)?;
                class.methods.get(lower).is_some_and(|method| {
                    method.visibility != Visibility::Private
                        || method.declaring == class.name
                        || first.is_object()
                }) || (class.name == "Closure" && name.eq_ignore_ascii_case("__invoke"))
            }
        })
    }
}

/// The lowercased form of `name` in `buffer`, when it fits: method and
/// property names are matched case-insensitively, and the general handler
/// lowercases them into a `String` per call.
fn lowercase64<'a>(name: &str, buffer: &'a mut [u8; 64]) -> Option<&'a str> {
    let bytes = name.as_bytes();
    let slot = buffer.get_mut(..bytes.len())?;
    slot.copy_from_slice(bytes);
    slot.make_ascii_lowercase();
    std::str::from_utf8(slot).ok()
}
