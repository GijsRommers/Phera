//! The class queries that need no PHP code, answered in the interpreter loop.
//!
//! ```php
//! class_exists('A');           // true when A is loaded; a miss may autoload it
//! class_exists('A', false);    // only asks the class table
//! interface_exists('Marker');  // the table says what kind of entry a name is
//! trait_exists('T'); enum_exists('S');
//! is_a($child, 'Marker');      // the object's class, its parents and interfaces
//! is_subclass_of($child, 'Base');
//! get_class($base);            // the object's class name
//! get_parent_class($child);    // its parent's name, or false
//! ```
//!
//! (The member queries `method_exists` / `property_exists` are in
//! `class_member_queries.rs`.)
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

use phera_bytecode::{ClassKind, Reg};
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

/// Which class-name function `get_class` / `get_parent_class` is, made by the
/// routing arm in `calls::sync_native`.
#[derive(Clone, Copy)]
pub(crate) enum ClassNameQuery {
    GetClass,
    GetParentClass,
}

/// What a string class name resolves to without running any PHP code.
pub(crate) enum ResolvedName {
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
        self.finish_query(answer.map(Value::Bool), call, destination)
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
        self.finish_query(answer.map(Value::Bool), call, destination)
    }

    /// `get_class` / `get_parent_class` for a plain call with one argument:
    /// the answer from the object's class or the table, or the exit for the
    /// general handler.
    #[inline(never)]
    pub(crate) fn class_name_call(
        &mut self,
        query: ClassNameQuery,
        call: PreparedCall,
        destination: Reg,
    ) -> Option<BatchExit> {
        let answer = self
            .plain_window(call, &[1])
            .and_then(|window| match window {
                [Some(first)] => self.class_name_answer(query, first),
                _ => None,
            });
        self.finish_query(answer, call, destination)
    }

    /// The argument window of a plain positional call with one of `counts`
    /// arguments and no named arguments, when the call has that shape.
    pub(crate) fn plain_window(
        &self,
        call: PreparedCall,
        counts: &[usize],
    ) -> Option<&[Option<Value>]> {
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
    pub(crate) fn finish_query(
        &mut self,
        answer: Option<Value>,
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
        self.set(destination, answer);
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

    /// The answer for `get_class` / `get_parent_class` with one argument:
    /// `Some(answer)` when no PHP code can change it, `None` to fall back.
    fn class_name_answer(&self, query: ClassNameQuery, first: &Value) -> Option<Value> {
        match query {
            ClassNameQuery::GetClass => {
                // The parameter is `object`, so only an object arrives plainly;
                // the class name is the object's own, and a lazy object stays
                // uninitialized, as in the general handler.
                let Value::Object(object) = first else {
                    return None;
                };
                Some(Value::from(object.class.name()))
            }
            ClassNameQuery::GetParentClass => {
                let class = match first {
                    Value::Object(object) => self.classes.for_object(object),
                    Value::String(name) => {
                        let name = std::str::from_utf8(name.as_bytes()).ok()?;
                        match self.resolve_name(name, true) {
                            ResolvedName::Found(class) => Some(class),
                            // A name that resolves to no class makes the general
                            // handler raise its TypeError.
                            ResolvedName::Absent | ResolvedName::Fallback => return None,
                        }
                    }
                    _ => return None,
                };
                let class = class?;
                Some(
                    class
                        .parent
                        .as_ref()
                        .map_or(Value::Bool(false), |parent| parent.name_value.clone()),
                )
            }
        }
    }

    /// Resolve a string class name the way the general lookup's first step
    /// does, without running PHP: the table, then (only when `autoload` is
    /// allowed) the loaders and bundled sources, which may run PHP code.
    pub(crate) fn resolve_name(&self, name: &str, autoload: bool) -> ResolvedName {
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
