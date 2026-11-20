//! `instanceof` with a class name, answered in the interpreter loop.
//!
//! ```php
//! if ($value instanceof Arrayable) { ... }   // InstanceOf, an interface
//! if ($this instanceof Model) { ... }        // InstanceOf, a parent class
//! ```
//!
//! The class the name means is the one the file linked the name to
//! (`UnitCache::classes`), or the running code's class for `self`,
//! `static` and `parent`. The answer follows the object's linked ancestry
//! by pointer: its class, its parents and the interfaces each one
//! implements. No name is hashed or lowercased.
//!
//! The loop leaves the instruction to the VM ([`Step::Slow`]) when the name
//! is not linked yet (the VM links it), when the operand is undefined or a
//! PHP reference, when the object's class is not a linked class, and when
//! the interfaces to walk do not fit the walk's fixed stack, and in the
//! guarded loop (some object has a destructor due).

use std::rc::Rc;

use phera_bytecode::{Reg, SymbolId};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::classes::Class;
use crate::fast::{ScalarStep, store};

/// How many classes the walk keeps waiting at once, and how many it
/// visits in all (an interface reached along two paths is visited twice).
/// Real hierarchies stay far below both; a larger one goes to the VM.
const WALK_DEPTH: usize = 32;
const WALK_VISITS: usize = 256;

impl Machine<'_> {
    /// `InstanceOf`, as `Vm::instance_of` answers it.
    pub(super) fn instance_of<const GUARDED: bool>(
        &mut self,
        position: Position,
        destination: Reg,
        source: Reg,
        symbol: SymbolId,
    ) -> Step {
        // A `catch` clause is an `instanceof` of the thrown object, reached
        // while the unwinding may have left destructors due; the VM's exit
        // runs them before the clause's code, as PHP does.
        if GUARDED {
            return Step::Slow;
        }
        let slot = position.frame.base() + source.index();
        let answer = match self.registers.window(slot, 1) {
            [Some(Value::Object(object))] => {
                let Some(class) = object.class.as_any().downcast_ref::<Class>() else {
                    return Step::Slow;
                };
                let Some(target) = self.instance_target(position, symbol) else {
                    return Step::Slow;
                };
                match derives_from(class, &target) {
                    Some(answer) => answer,
                    None => return Step::Slow,
                }
            }
            // Only an object is an instance; `self` outside a class is an
            // error the VM raises.
            [Some(Value::Reference(_))] | [None] | [] => return Step::Slow,
            [Some(_)] if self.relative_symbol(position, symbol) => return Step::Slow,
            [Some(_)] => false,
            _ => return Step::Slow,
        };
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED>(slots, destination, Value::Bool(answer)) {
            ScalarStep::Complete => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    /// The class `symbol` names at this point of the running code.
    fn instance_target(&self, position: Position, symbol: SymbolId) -> Option<Rc<Class>> {
        let function = position.frame.function;
        let written = &self
            .code
            .unit(function)
            .symbols
            .get(symbol.index())?
            .written;
        let context = self.current_context();
        match written.as_str() {
            "self" => context?.scope().cloned(),
            "static" => context?.called(),
            "parent" => context?.scope()?.parent.clone(),
            _ => self
                .code
                .cache(function)
                .classes
                .get(symbol.index())?
                .borrow()
                .clone(),
        }
    }

    /// Whether `symbol` is `self`, `static` or `parent`.
    fn relative_symbol(&self, position: Position, symbol: SymbolId) -> bool {
        self.code
            .unit(position.frame.function)
            .symbols
            .get(symbol.index())
            .is_none_or(|symbol| matches!(symbol.written.as_str(), "self" | "static" | "parent"))
    }
}

/// Whether `class` is `target`, extends it or implements it: the same
/// ancestry as `Class::ancestors`, its parent and interfaces and theirs.
/// `None` when the walk outgrows its stack or its visits.
fn derives_from(class: &Class, target: &Class) -> Option<bool> {
    let mut waiting: [Option<&Class>; WALK_DEPTH] = [None; WALK_DEPTH];
    waiting[0] = Some(class);
    let mut count: usize = 1;
    for _ in 0..WALK_VISITS {
        let Some(last) = count.checked_sub(1) else {
            return Some(false);
        };
        count = last;
        let class = waiting.get(last).copied().flatten()?;
        if std::ptr::eq(class, target) {
            return Some(true);
        }
        for ancestor in class.parent.iter().chain(&class.interfaces) {
            *waiting.get_mut(count)? = Some(&**ancestor);
            count += 1;
        }
    }
    None
}
