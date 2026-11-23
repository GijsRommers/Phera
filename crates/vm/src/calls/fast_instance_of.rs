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

use phera_bytecode::{Reg, SymbolId, Target};
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
        let Some(answer) = self.instance_of_answer::<GUARDED>(position, source, symbol) else {
            return Step::Slow;
        };
        let slots = self.registers.tail_mut(position.frame.base());
        match store::<GUARDED, _>(slots, destination, Value::Bool(answer)) {
            ScalarStep::Complete | ScalarStep::Released => Step::Done(position.ip),
            ScalarStep::Fallback => Step::Slow,
        }
    }

    /// `InstanceOfBranch` / `InstanceOfBranchIf`: the same answer, jumping to
    /// `target` when it equals `when` instead of storing it.
    pub(super) fn instance_of_branch<const GUARDED: bool>(
        &mut self,
        position: Position,
        source: Reg,
        symbol: SymbolId,
        target: Target,
        when: bool,
    ) -> Step {
        let Some(answer) = self.instance_of_answer::<GUARDED>(position, source, symbol) else {
            return Step::Slow;
        };
        if answer == when {
            if target.index() < position.ip && self.spend_work() {
                return Step::Yield;
            }
            return Step::Done(target.index());
        }
        Step::Done(position.ip)
    }

    /// The `instanceof` answer for a resolved class name, or `None` when the
    /// VM must answer it.
    fn instance_of_answer<const GUARDED: bool>(
        &mut self,
        position: Position,
        source: Reg,
        symbol: SymbolId,
    ) -> Option<bool> {
        // A `catch` clause is an `instanceof` of the thrown object, reached
        // while the unwinding may have left a destructor waiting; the VM's
        // exit runs it before the clause's code, as PHP does.
        if GUARDED && self.ids.has_doomed() {
            return None;
        }
        let slot = position.frame.base() + source.index();
        match self.registers.window(slot, 1) {
            [Some(Value::Object(object))] => {
                let class = object.class.as_any().downcast_ref::<Class>()?;
                let function = position.frame.function;
                let key = self.code.unit_symbol_key(function, symbol);
                // Fast path: non-relative loaded class — no Rc clone, no written-string scan.
                if let Some(target) = self.classes.by_key(key) {
                    return class.derives_from(target);
                }
                let written = &self
                    .code
                    .unit(function)
                    .symbols
                    .get(symbol.index())?
                    .written;
                match written.as_str() {
                    "self" => class.derives_from(self.caller_scope(position)?),
                    "parent" => class.derives_from(self.caller_scope(position)?.parent.as_deref()?),
                    "static" => {
                        let called = self.current_context()?.called()?;
                        class.derives_from(&called)
                    }
                    // A name no class of this request has: nothing is an
                    // instance of it, and `instanceof` never autoloads.
                    _ => Some(false),
                }
            }
            // Only an object is an instance; `self` outside a class is an
            // error the VM raises.
            [Some(Value::Reference(_))] | [None] | [] => None,
            [Some(_)] => {
                let function = position.frame.function;
                let key = self.code.unit_symbol_key(function, symbol);
                if self.classes.by_key(key).is_some() || !self.relative_symbol(position, symbol) {
                    Some(false)
                } else {
                    None
                }
            }
            _ => None,
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
