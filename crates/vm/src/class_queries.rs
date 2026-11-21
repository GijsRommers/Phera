//! The class queries that need no PHP code, answered in the interpreter loop.
//!
//! ```php
//! class_exists('A');           // true when A is loaded; a miss may autoload it
//! class_exists('A', false);    // only asks the class table
//! interface_exists('Marker');  // the table says what kind of entry a name is
//! trait_exists('T'); enum_exists('S');
//! is_a($child, 'Marker');      // the object's class, its parents and interfaces
//! is_subclass_of($child, 'Base');
//! method_exists($base, 'pubMethod');   // the class's method table
//! property_exists($base, 'pub');       // declared, static and own properties
//! ```
//!
//! The general handler ([`Vm::class_intrinsic`]) is the full behaviour: it
//! coerces arguments, runs the autoloader for a miss when the call allows it,
//! and raises PHP's errors. A plain positional call whose arguments already
//! have their exact types (a string or object, a string, a bool flag) needs
//! no PHP code, so the loop answers it from the call's argument window, the
//! way `spl_object_id` is answered: names are looked up in the class table
//! (which strips a leading backslash and lowercases into a stack buffer,
//! without allocating). Every other shape — a named argument, an argument
//! still waiting for its coercion, a name that is not valid UTF-8, too few
//! or too many arguments — returns `BatchExit::NativeCall` with the window
//! intact, for the general handler.

use std::rc::Rc;

use phera_bytecode::{ClassKind, Reg, Visibility};
use phera_value::{PhpString, Value};

use crate::Vm;
use crate::bundled_classes::is_bundled_class_name;
use crate::calls::execution_loop::BatchExit;
use crate::calls::prepared_call::PreparedCall;
use crate::classes::Class;

/// Which entry kind a `*_exists` call asks for, made by the routing arm in
/// `calls::sync_native`.
#[derive(Clone, Copy)]
pub(crate) enum ExistsKind {
    Class,
    Interface,
    Trait,
    Enum,
}

impl ExistsKind {
    /// Whether a class table entry of `kind` answers the call.
    fn answers(self, kind: ClassKind) -> bool {
        match self {
            Self::Class => kind.is_class(),
            Self::Interface => kind == ClassKind::Interface,
            Self::Trait => kind == ClassKind::Trait,
            Self::Enum => kind == ClassKind::Enum,
        }
    }
}

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

/// Which member `method_exists` / `property_exists` asks for, made by the
/// routing arm in `calls::sync_native`.
#[derive(Clone, Copy)]
pub(crate) enum MemberQuery {
    Method,
    Property,
}

/// What a string class name resolves to without running any PHP code.
enum ResolvedName {
    /// The class table has the name.
    Found(Rc<Class>),
    /// No loader is registered and no bundled source can have the name, so
    /// the general lookup also answers "no class" without running PHP.
    Absent,
    /// The general handler may load the class: fall back to it.
    Fallback,
}

impl Vm {
    /// The four `*_exists` functions for a plain call: the answer from the
    /// class table, or the exit for the general handler.
    #[inline(never)]
    pub(crate) fn class_query_call(
        &mut self,
        kind: ExistsKind,
        call: PreparedCall,
        destination: Reg,
    ) -> Option<BatchExit> {
        let answer = self
            .plain_window(call, &[1, 2])
            .and_then(|window| match window {
                [Some(Value::String(name))] => self.exists_answer(kind, name, true),
                [Some(Value::String(name)), Some(Value::Bool(autoload))] => {
                    self.exists_answer(kind, name, *autoload)
                }
                _ => None,
            });
        self.finish_query(answer, call, destination)
    }

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
                [Some(first), Some(Value::String(target))] => {
                    self.relation_answer(query, first, target, query.default_allow_string())
                }
                [
                    Some(first),
                    Some(Value::String(target)),
                    Some(Value::Bool(allow_string)),
                ] => self.relation_answer(query, first, target, *allow_string),
                _ => None,
            });
        self.finish_query(answer, call, destination)
    }

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
                [Some(first), Some(Value::String(name))] => self.member_answer(query, first, name),
                _ => None,
            });
        self.finish_query(answer, call, destination)
    }

    /// The argument window of a plain positional call with one of `counts`
    /// arguments and no named arguments, when the call has that shape.
    fn plain_window(&self, call: PreparedCall, counts: &[usize]) -> Option<&[Option<Value>]> {
        let count = call.sent as usize;
        if !call.is_plain()
            || !counts.contains(&count)
            || !self
                .call_stack
                .top_details()
                .is_some_and(|details| details.named.is_none() && details.extra_named.is_empty())
        {
            return None;
        }
        let window = self.registers.window(call.base(), count);
        (window.len() == count).then_some(window)
    }

    /// Store a computed answer, or exit for the general handler when the call
    /// has a shape this module cannot answer.
    fn finish_query(
        &mut self,
        answer: Option<bool>,
        call: PreparedCall,
        destination: Reg,
    ) -> Option<BatchExit> {
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
    /// `Some(answer)` when no PHP code can change it, `None` to fall back.
    fn exists_answer(&self, kind: ExistsKind, name: &PhpString, autoload: bool) -> Option<bool> {
        // The general handler lossy-converts bytes that are not UTF-8 before
        // looking the name up; only a valid name takes this path.
        let name = std::str::from_utf8(name.as_bytes()).ok()?;
        match self.resolve_name(name, autoload) {
            ResolvedName::Found(class) => Some(kind.answers(class.kind)),
            ResolvedName::Absent => Some(false),
            ResolvedName::Fallback => None,
        }
    }

    /// The answer for an `is_a` / `is_subclass_of` call whose arguments are
    /// `first` (an object or a class name), `target` (a class name) and
    /// `allow_string`.
    fn relation_answer(
        &self,
        query: RelationQuery,
        first: &Value,
        target: &PhpString,
        allow_string: bool,
    ) -> Option<bool> {
        // Only a valid UTF-8 target takes this path, for the same reason as
        // in `exists_answer`.
        let target = std::str::from_utf8(target.as_bytes()).ok()?;
        let class = match first {
            // An object's class is known; PHP never autoloads it.
            Value::Object(object) => self.classes.for_object(object),
            Value::String(name) => {
                let name = std::str::from_utf8(name.as_bytes()).ok()?;
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
            _ => return None,
        };
        // The second argument is only looked up in the table; PHP never
        // autoloads it.
        let target_class = self.classes.get(target);
        Some(class.zip(target_class).is_some_and(|(class, target)| {
            class.is_a(&target.name)
                && !(matches!(query, RelationQuery::IsSubclassOf) && class.index == target.index)
        }))
    }

    /// The answer for a `method_exists` / `property_exists` call whose
    /// arguments are `first` (an object or a class name) and `name`.
    fn member_answer(&self, query: MemberQuery, first: &Value, name: &PhpString) -> Option<bool> {
        // Only a valid UTF-8 name takes this path, for the same reason as in
        // `exists_answer`.
        let name = std::str::from_utf8(name.as_bytes()).ok()?;
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
            Value::String(class_name) => {
                let class_name = std::str::from_utf8(class_name.as_bytes()).ok()?;
                match self.resolve_name(class_name, true) {
                    ResolvedName::Found(class) => Some(class),
                    ResolvedName::Absent => None,
                    ResolvedName::Fallback => return None,
                }
            }
            // The general handler raises its TypeError for other types.
            _ => return None,
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

    /// Resolve a string class name the way the general lookup's first step
    /// does, without running PHP: the table, then (only when `autoload` is
    /// allowed) the loaders and bundled sources, which may run PHP code.
    fn resolve_name(&self, name: &str, autoload: bool) -> ResolvedName {
        if let Some(class) = self.classes.get(name) {
            return ResolvedName::Found(class);
        }
        if !autoload {
            return ResolvedName::Absent;
        }
        let callback_name = name.strip_prefix('\\').unwrap_or(name);
        if self.has_autoloaders() || is_bundled_class_name(callback_name) {
            return ResolvedName::Fallback;
        }
        ResolvedName::Absent
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
