//! Property reads and writes that the interpreter loop makes without the VM.
//!
//! ```php
//! class Point {
//!     public function __construct(private int $x) {}
//!     public function x(): int { return $this->x; }   // Fetch $this->x
//!     public function move(int $by): void {
//!         $this->x = $this->x + $by;                  // AssignPath $this->x
//!         $this->moves++;                             // IncDecPath $this->moves
//!     }
//! }
//! ```
//!
//! The first read at an instruction goes through the VM, which checks
//! visibility, finds where the property is stored and records that in the
//! instruction's property site (`crate::property_sites`), for the object's
//! class and the class scope of the code that reads. A later read at the
//! same instruction, of an object of the same class from the same scope,
//! needs only that record: this module reads or writes the property here,
//! inside the loop. Anything else (another class, an uninitialized or hooked
//! property, a magic `__get` or `__set`, a value that needs a type
//! coercion, `$this` outside an object) answers [`Step::Slow`] without
//! having changed anything.
//!
//! A write drops the value the property held. While some object has a
//! destructor due (the guarded loop, see `crate::destructors`) that could
//! free it, so writes are then left to the VM, which runs the destructor
//! right after.

use std::rc::Rc;

use phera_bytecode::{IncDec, PathBase, PathId, Reg, Step as PathStep};
use phera_value::{Object, Value};

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::classes::Class;
use crate::fast::{ScalarStep, store};
use crate::property_sites::PropertyEntry;
use crate::unit_caches::CachedSite;

impl Machine<'_> {
    /// `Fetch` in read mode of a path that is one declared property of
    /// `$this` or of an object in a register.
    #[inline(never)]
    pub(super) fn fetch_property<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        path: PathId,
    ) -> Step {
        let Some(value) = self.cached_property(position, path) else {
            return Step::Slow;
        };
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED>(slots, destination, value) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    /// `Fetch` of one declared property in a quiet mode (`??`, `isset`,
    /// `empty`). Unlike a plain read, PHP calls a `get` hook first here,
    /// so a property with hooks is left to the VM.
    pub(super) fn fetch_property_quietly<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        path: PathId,
    ) -> Step {
        let Some(value) = self.with_site(position, (path, Shape::Read), |entry, object, scope| {
            if entry.hooks.is_some() {
                return None;
            }
            entry.read_matching(object, scope)
        }) else {
            return Step::Slow;
        };
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED>(slots, destination, value) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    /// `AssignPath`, or with `result` `AssignPathFromLocal`, of a value to
    /// one declared property, outside the guarded loop.
    #[inline(never)]
    pub(super) fn assign_property<const GUARDED: bool>(
        &mut self,
        position: Position,
        path: PathId,
        (source, result): (Reg, Option<Reg>),
    ) -> Step {
        let base = position.frame.base();
        // Once the property is written, the value must be stored.
        if result.is_some_and(|result| self.registers.window(base + result.index(), 1).is_empty()) {
            return Step::Slow;
        }
        let value = match self.registers.window(base + source.index(), 1) {
            [Some(value)] => value.deref().clone(),
            _ => return Step::Slow,
        };
        let written = self.with_site(position, (path, Shape::Write), |entry, object, scope| {
            entry.write_matching_as(object, scope, &value)
        });
        let Some(converted) = written else {
            return Step::Slow;
        };
        if let Some(result) = result {
            // The assignment's result is the value stored: `1.0` for a
            // `float` property assigned `1`. Outside the guarded loop, into
            // a register checked above, a store completes.
            let stored = if converted {
                converted_to_float(value)
            } else {
                value
            };
            let _ = store::<false>(self.registers.tail_mut(base), result, stored);
        }
        Step::Done(position.ip)
    }

    /// `IncDecPath` of one declared integer property.
    #[inline(never)]
    pub(super) fn increment_property<const GUARDED: bool>(
        &mut self,
        position: Position,
        (mode, destination): (IncDec, Reg),
        path: PathId,
    ) -> Step {
        // Once the property has changed, the result must be stored here,
        // so the register is checked first: in the guarded loop, writing
        // over an array or object could free one with a destructor.
        let slot = position.frame.base() + destination.index();
        match self.registers.window(slot, 1) {
            [_] => {}
            _ => return Step::Slow,
        }
        let Some(value) = self.with_site(position, (path, Shape::Write), |entry, object, scope| {
            entry.increment_matching(object, scope, mode)
        }) else {
            return Step::Slow;
        };
        self.registers.put(slot, value);
        Step::Done(position.ip)
    }

    /// The value the instruction's property site gives for this read.
    pub(super) fn cached_property(&self, position: Position, path: PathId) -> Option<Value> {
        self.with_site(position, (path, Shape::Read), |entry, object, scope| {
            entry.read_matching(object, scope)
        })
    }

    /// Runs `access` with the instruction's property site, the object the
    /// path reaches and the class scope of the running code, when the path
    /// is one declared property (of `$this`, of an object in a register,
    /// or for a read, of an object at an integer index of an array) and
    /// the site has an entry.
    pub(super) fn with_site<R>(
        &self,
        position: Position,
        (path, shape): (PathId, Shape),
        access: impl FnOnce(&PropertyEntry, &Object, Option<&Rc<Class>>) -> Option<R>,
    ) -> Option<R> {
        let function = position.frame.function;
        let body = self.code.function(function);
        let path = body.paths.get(path.index())?;
        let object = match (path.steps.as_slice(), path.base, shape) {
            ([PathStep::Property(_)], base, _) => self.property_object(position, base)?,
            (
                [PathStep::Index(index), PathStep::Property(_)],
                PathBase::Register(root),
                Shape::Read,
            ) => self.element_object(position, root, *index)?,
            (
                [
                    PathStep::Property(_),
                    PathStep::Index(_) | PathStep::Append,
                    ..,
                ],
                base,
                Shape::Element,
            ) => self.property_object(position, base)?,
            _ => return None,
        };
        // The VM keys the site by the position after the instruction,
        // which is where the loop stands now.
        let site = self
            .code
            .cache(function)
            .site(self.code.facts(function), position.ip)?
            .try_borrow()
            .ok()?;
        let CachedSite::Property(entry) = &*site else {
            return None;
        };
        let scope = self.current_context().and_then(|context| context.scope());
        access(entry, object, scope)
    }

    /// The object at integer index `index` of the array in register `root`
    /// (`$items[$i]->key`), as the VM's indexed property reads find it.
    fn element_object(&self, position: Position, root: Reg, index: Reg) -> Option<&Rc<Object>> {
        let base = position.frame.base();
        let [Some(Value::Int(index))] = self.registers.window(base + index.index(), 1) else {
            return None;
        };
        let [Some(Value::Array(array))] = self.registers.window(base + root.index(), 1) else {
            return None;
        };
        match crate::array_slots::read_integer(array, *index)? {
            Value::Object(object) => Some(object),
            _ => None,
        }
    }

    /// The object a property path starts from.
    fn property_object(&self, position: Position, base: PathBase) -> Option<&Rc<Object>> {
        match base {
            PathBase::This { .. } => self.current_context()?.this(),
            PathBase::Register(register) => {
                match self
                    .registers
                    .window(position.frame.base() + register.index(), 1)
                {
                    [Some(Value::Object(object))] => Some(object),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// Which paths an access takes: the VM writes only to a property reached
/// directly, but reads also one of an object found at an array index; an
/// element read starts with a property and goes on with array keys.
#[derive(Clone, Copy)]
pub(super) enum Shape {
    Read,
    Write,
    Element,
}

/// The float an integer written to a `float` property is stored as.
#[cold]
#[inline(never)]
fn converted_to_float(value: Value) -> Value {
    match value {
        Value::Int(integer) => Value::Float(integer as f64),
        value => value,
    }
}
