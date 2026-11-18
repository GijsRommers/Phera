//! Default and by-reference arguments in the interpreter loop.
//!
//! ```php
//! function step(int $value, int $by = 1, ?string $unit = null) { ... }
//! step($i);                // ReceiveDefault, LoadConst, CheckType
//! sort_in_place($items);   // SendLocal to a by-reference parameter
//! ```
//!
//! A parameter with a default starts with `ReceiveDefault`: if the caller
//! passed the argument, it jumps past the code that computes the default.
//! That code is ordinary instructions; a constant default is `LoadConst`
//! and, for a typed parameter, `CheckType`, which here only has to confirm
//! that the value already has the type. A local sent to a by-reference
//! parameter becomes a PHP reference (if it is not one yet) shared by the
//! caller's variable and the callee's parameter.

use std::cell::RefCell;
use std::rc::Rc;

use phera_bytecode::{ConstId, Reg, Target, TypeId};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::prepared_call::PreparedCallee;

impl Machine<'_> {
    /// `ReceiveDefault`: skip the default's code when the argument was passed.
    #[inline(always)]
    pub(super) fn receive_default(
        &self,
        position: Position,
        parameter: Reg,
        passed: Target,
    ) -> Step {
        let slot = position.frame.base() + parameter.index();
        match self.registers.window(slot, 1) {
            [Some(_)] => Step::Done(passed.index()),
            _ => Step::Done(position.ip),
        }
    }

    /// `LoadConst`: copy a literal of the file into a register.
    #[inline(always)]
    pub(super) fn load_constant(
        &mut self,
        position: Position,
        dst: Reg,
        constant: ConstId,
    ) -> Step {
        let cache = self.code.cache(position.frame.function);
        let Some(value) = cache.constants.get(constant.index()) else {
            return Step::Slow;
        };
        self.registers
            .put(position.frame.base() + dst.index(), value.clone());
        Step::Done(position.ip)
    }

    /// `CheckType` of a local that has the declared type already. A
    /// conversion, a class type or a frame with `foreach` state takes the
    /// general path.
    #[inline(always)]
    pub(super) fn check_type(&self, position: Position, local: Reg, ty: TypeId) -> Step {
        let frame = position.frame;
        let declared = &self.code.unit(frame.function).types[ty.index()];
        let accepted = match self.registers.window(frame.base() + local.index(), 1) {
            [Some(value)] => crate::types::accepts_as_is(value, declared),
            _ => false,
        };
        match accepted && !frame.has_extras() {
            true => Step::Done(position.ip),
            false => Step::Slow,
        }
    }

    /// `SendLocal` to an untyped by-reference parameter of a PHP function,
    /// from a function's own local: the local becomes a reference if it is
    /// not one, and the parameter shares it. Top-level code (whose
    /// variables may be globals), frames with `foreach` state and typed
    /// parameters take the general path.
    #[inline(always)]
    pub(super) fn send_reference(&mut self, position: Position, local: Reg) -> Step {
        let Some(call) = self.stack.last_call().copied() else {
            return Step::Slow;
        };
        let PreparedCallee::Php(function) = call.callee else {
            return Step::Slow;
        };
        let index = call.sent as usize;
        let frame = position.frame;
        let facts = self.code.facts(function);
        if !facts.takes_reference(index)
            || facts.is_typed(index)
            || call.flags != Default::default()
            || frame.has_extras()
            || self.code.local(frame.function).is_none()
        {
            return Step::Slow;
        }
        let slot = frame.base() + local.index();
        let reference = match self.registers.window(slot, 1) {
            [Some(Value::Reference(cell))] => Value::Reference(cell.clone()),
            [value] => {
                let value = value.clone().unwrap_or_default();
                let reference = Value::Reference(Rc::new(RefCell::new(value)));
                self.registers.put(slot, reference.clone());
                reference
            }
            _ => return Step::Slow,
        };
        self.registers.put(call.base() + index, reference);
        if let Some(call) = self.stack.last_call_mut() {
            call.mark_sent(index, true);
        }
        Step::Done(position.ip)
    }
}
