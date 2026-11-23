//! Reads of a path with several property steps, in the interpreter loop.
//!
//! ```php
//! $mapping = $this->rsm->fieldMappings[$key];    // Fetch, reading
//! if (isset($this->rsm->fieldMappings[$key])) {} // Fetch, Isset
//! $this->load($this->rsm->declaringClasses[$key]); // SendPath
//! ```
//!
//! The VM records one site entry per property step of such a path
//! (`crate::property_chains`). The loop walks the path with them: a property
//! step reads through its entry, which must match the object's class and the
//! caller's scope, and a key step reads the array. A read gives the value at
//! the end; a quiet read (`??`, `isset`, `empty`) gives null when a key is
//! missing or a level is null. Anything else (a missing key or a null level
//! in a plain read, which warn; a property the entry does not answer; a
//! lazy object, which a read initializes; an object where a key follows; a
//! PHP reference) answers [`Step::Slow`] with nothing changed.

use phera_bytecode::{FetchMode, PathBase, PathId, Reg, Step as PathStep};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::fast::{ScalarStep, store};
use crate::unit_caches::CachedSite;

impl Machine<'_> {
    /// `Fetch` of a path with several property steps, in `mode`.
    #[inline(never)]
    pub(super) fn fetch_property_chain<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        (path, mode): (PathId, FetchMode),
    ) -> Step {
        let quiet = match mode {
            FetchMode::Read => false,
            FetchMode::Quiet | FetchMode::Isset | FetchMode::Empty => true,
            FetchMode::Destructure => return Step::Slow,
        };
        let Some(value) = self.property_chain(position, path, quiet) else {
            return Step::Slow;
        };
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED, _>(slots, destination, value) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    /// The value at the end of a chain path, through the entries its site
    /// recorded; `None` when the path or a step is not one the loop takes.
    pub(super) fn property_chain(
        &self,
        position: Position,
        path: PathId,
        quiet: bool,
    ) -> Option<Value> {
        let function = position.frame.function;
        let body = self.code.function(function).paths.get(path.index())?;
        // The VM keys the site by the position after the instruction,
        // which is where the loop stands now.
        let site = self.code.site(function, position.ip)?.try_borrow().ok()?;
        let CachedSite::PropertyChain(entries) = &*site else {
            return None;
        };
        let mut current = match body.base {
            PathBase::This { .. } => Value::Object(self.current_context()?.this()?.clone()),
            PathBase::Register(register) => match self
                .registers
                .window(position.frame.base() + register.index(), 1)
            {
                [Some(value @ (Value::Object(_) | Value::Array(_)))] => value.clone(),
                _ => return None,
            },
            _ => return None,
        };
        let scope = self.current_context().and_then(|context| context.scope());
        let mut entries = entries.iter();
        for step in &body.steps {
            current = match (step, &current) {
                (PathStep::Property(_), Value::Object(object)) => {
                    let entry = entries.next()?;
                    if entry.hooks.is_some() || object.lazy().is_some() {
                        return None;
                    }
                    entry.read_matching(object, scope)?
                }
                (PathStep::Index(register), Value::Array(array)) => {
                    let key = self.array_key(position, *register)?;
                    match array.get(&key) {
                        Some(Value::Reference(_)) => return None,
                        Some(element) => element.clone(),
                        None if quiet => return Some(Value::Null),
                        None => return None,
                    }
                }
                (PathStep::Property(_) | PathStep::Index(_), Value::Null) if quiet => {
                    return Some(Value::Null);
                }
                _ => return None,
            };
        }
        Some(current)
    }
}
