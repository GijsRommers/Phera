//! `instanceof` with a class name, answered in the interpreter loop.
//!
//! ```php
//! if ($value instanceof Arrayable) { ... }   // InstanceOf, an interface
//! if ($this instanceof Model) { ... }        // InstanceOf, a parent class
//! ```
//!
//! The class the name means is the request's class for the name's key
//! (`ClassTable::by_key`), or the running code's class for `self`,
//! `static` and `parent`. The answer follows the object's linked ancestry
//! by pointer: its class, its parents and the interfaces each one
//! implements. No name is hashed or lowercased.
//!
//! A name that means no class of the request (`$entity instanceof Proxy`
//! where no proxy class was ever loaded) answers `false`, as PHP does
//! without autoloading. The loop leaves the instruction to the VM
//! ([`Step::Slow`]) when the operand is undefined or a
//! PHP reference, when the object's class is not a linked class, and when
//! the interfaces to walk do not fit the walk's fixed stack, and in the
//! guarded loop (some object has a destructor due).

use std::rc::Rc;

use phera_bytecode::{Reg, SymbolId};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::classes::Class;
use crate::fast::{ScalarStep, store};

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
        // while the unwinding may have left a destructor waiting; the VM's
        // exit runs it before the clause's code, as PHP does.
        if GUARDED && self.ids.has_doomed() {
            return Step::Slow;
        }
        let slot = position.frame.base() + source.index();
        let answer = match self.registers.window(slot, 1) {
            [Some(Value::Object(object))] => {
                let Some(class) = object.class.as_any().downcast_ref::<Class>() else {
                    return Step::Slow;
                };
                match self.instance_target(position, symbol) {
                    Some(target) => match class.derives_from(&target) {
                        Some(answer) => answer,
                        None => return Step::Slow,
                    },
                    // A name no class of this request has: nothing is an
                    // instance of it, and `instanceof` never autoloads.
                    None if !self.relative_symbol(position, symbol) => false,
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
        match store::<GUARDED, _>(slots, destination, Value::Bool(answer)) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
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
            _ => self.linked_class(position, symbol),
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
