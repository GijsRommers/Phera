//! Property reads that the interpreter loop makes without the VM.
//!
//! ```php
//! class Point {
//!     public function __construct(private int $x) {}
//!     public function x(): int { return $this->x; }   // Fetch $this->x
//! }
//! echo $point->x();
//! ```
//!
//! The first read at an instruction goes through the VM, which checks
//! visibility, finds where the property is stored and records that in the
//! instruction's property site (`crate::property_sites`), for the object's
//! class and the class scope of the code that reads. A later read at the
//! same instruction, of an object of the same class from the same scope,
//! needs only that record: this module reads the property here, inside
//! the loop. Anything else (another class, an uninitialized or hooked
//! property, a magic `__get`, `$this` outside an object) answers
//! [`Step::Slow`] without having changed anything.

use std::rc::Rc;

use phera_bytecode::{PathBase, PathId, Reg, Step as PathStep};
use phera_value::{Object, Value};

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::fast::{ScalarStep, store};
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
            ScalarStep::Complete => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    /// The value the instruction's property site gives for this read.
    fn cached_property(&self, position: Position, path: PathId) -> Option<Value> {
        let function = position.frame.function;
        let path = self.code.function(function).paths.get(path.index())?;
        let [PathStep::Property(_)] = path.steps.as_slice() else {
            return None;
        };
        let object = self.property_object(position, path.base)?;
        // The VM keys the site by the position after the instruction,
        // which is where the loop stands now.
        let site = self
            .code
            .cache(function)
            .site(self.code.local(function), position.ip)?
            .try_borrow()
            .ok()?;
        let CachedSite::Property(entry) = &*site else {
            return None;
        };
        let scope = self.current_context().and_then(|context| context.scope());
        entry.read_matching(object, scope)
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
