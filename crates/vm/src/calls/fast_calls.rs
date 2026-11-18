//! Calls and returns that the interpreter loop makes without the VM.
//!
//! ```php
//! function add(int $a, int $b): int { return $a + $b; }   // CallDirect
//! $sum += $point->add($i, 2);                              // InitMethodCall, Send, Call
//! ```
//!
//! Each step here checks that its case is the plain one (a public method
//! found in the class's table, an argument that passes its parameter type's
//! bit test, a frame with nothing unusual to release) and otherwise answers
//! [`Step::Slow`] without having changed anything, so that `Vm::execute`
//! runs the instruction with every PHP rule.

use phera_bytecode::{FunctionId, Reg};
use phera_value::Value;

use crate::calls::call_state::CallTarget;
use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::frame::Frame;
use crate::calls::prepared_call::{CallFlags, CallKind, PreparedCallee};
use crate::code_table::{FunctionIndex, FunctionShape};

impl Machine<'_> {
    /// `CallDirect` to a plain function with valid arguments: move the
    /// arguments into a new window at the top of the register stack and
    /// push a frame. Anything unusual (too few arguments, an argument that
    /// fails its type's bit test, the frame limit) takes the general path.
    #[inline(always)]
    pub(super) fn enter_direct(
        &mut self,
        caller: Position,
        declared: FunctionId,
        (args, count): (Reg, u16),
        destination: Reg,
    ) -> Step {
        let first = self.code.facts(caller.frame.function).unit_first;
        let Some((function, facts)) = self.code.declared_facts(first, declared.index()) else {
            return Step::Slow;
        };
        let count = count as usize;
        if facts.shape != FunctionShape::Plain
            || count < facts.required as usize
            || count > facts.parameters as usize
            || self.stack.depth() >= self.frame_limit
        {
            return Step::Slow;
        }
        let from = caller.frame.base() + args.index();
        let arguments = self.registers.window(from, count);
        let acceptable = arguments.iter().enumerate().all(|(index, argument)| {
            argument.as_ref().is_some_and(|value| {
                !value.is_reference()
                    && (!facts.is_typed(index)
                        || facts.parameter_masks[index].intersects(crate::types::own_type(value)))
            })
        });
        if !acceptable {
            return Step::Slow;
        }
        let Some(base) = self
            .registers
            .enter_window(from, count, facts.registers as usize)
        else {
            return Step::Slow;
        };
        self.publish(caller.ip);
        let mut frame = Frame::new(function, base, Some(destination));
        if count < facts.parameters as usize {
            frame = frame.with_passed(count);
        }
        self.stack.push(frame);

        self.spend_work();
        Step::Switched(Position { frame, ip: 0 })
    }

    /// `Return` from a function or method frame that keeps nothing else in
    /// the side table, returns to PHP code, and whose value passes its
    /// return type's bit test: write the value (for a constructor, the new
    /// object) into the caller's register and empty the callee's.
    #[inline(always)]
    pub(super) fn leave_plain(&mut self, source: Option<Reg>) -> Step {
        let Some(frame) = self.stack.current().copied() else {
            return Step::Slow;
        };
        let facts = self.code.facts(frame.function);
        let Some(destination) = frame.destination() else {
            return Step::Slow;
        };
        let depth = self.stack.depth();
        let kind = frame.kind();
        let start = frame.base() - kind.prefix();
        if depth <= self.stop + 1
            || facts.local.is_none()
            || (frame.has_extras() && !self.stack.extras(depth - 1).holds_only_called_class())
            || self.stack.has_calls_from(start)
        {
            return Step::Slow;
        }
        let Some(caller) = self.stack.caller().copied() else {
            return Step::Slow;
        };
        let value = match source {
            Some(source) => match self.registers.window(frame.base() + source.index(), 1) {
                [Some(value)] if !value.is_reference() => value,
                _ => return Step::Slow,
            },
            None => &Value::Null,
        };
        if let Some(mask) = facts.return_mask
            && !mask.intersects(crate::types::own_type(value))
            && !self
                .code
                .function(frame.function)
                .return_type
                .is_some_and(|declared| {
                    crate::class_types::known_class_match(
                        self.code.cache(frame.function),
                        declared,
                        value,
                    )
                })
        {
            return Step::Slow;
        }
        let value = match (kind, source) {
            (CallKind::Constructor, _) => self.registers.take(frame.base() - 1),
            (_, Some(source)) => self.registers.take(frame.base() + source.index()),
            (_, None) => Value::Null,
        };
        self.stack.pop();
        self.registers.truncate(start);
        self.registers
            .put(caller.base() + destination.index(), value);
        Step::Switched(Position {
            frame: caller,
            ip: caller.ip(),
        })
    }
}

impl Machine<'_> {
    /// `SendValue` or `SendLocal` to a by-value parameter whose declared
    /// type accepts the value as it is: write it into the window.
    #[inline(always)]
    pub(super) fn send(&mut self, position: Position, source: Reg) -> Step {
        let Some(call) = self.stack.last_call().copied() else {
            return Step::Slow;
        };
        let index = call.sent as usize;
        let value = match self
            .registers
            .window(position.frame.base() + source.index(), 1)
        {
            [Some(value)] => value.deref().clone(),
            _ => return Step::Slow,
        };
        let accepted = match call.callee {
            PreparedCallee::Php(function) => {
                !call.flags.contains(CallFlags::DETAILS)
                    && self.accepts_argument(function, index, &value)
            }
            PreparedCallee::General => self.general_accepts(index),
            PreparedCallee::Constructed => false,
        };
        if !accepted {
            return Step::Slow;
        }
        let Some(call) = self.stack.last_call_mut() else {
            return Step::Slow;
        };
        self.registers.put(call.base() + index, value);
        call.mark_sent(index, true);
        Step::Done(position.ip)
    }

    /// Whether parameter `index` of `function` takes `value` as it is, by
    /// value: no declared type, a built-in type the value has, or a class
    /// type the unit's cache knows the value's class satisfies.
    fn accepts_argument(&self, function: FunctionIndex, index: usize, value: &Value) -> bool {
        let facts = self.code.facts(function);
        if !facts.takes_value(index) {
            return false;
        }
        if !facts.is_typed(index)
            || facts.parameter_masks[index].intersects(crate::types::own_type(value))
        {
            return true;
        }
        let declared = self.code.function(function).parameters[index].ty;
        declared.is_some_and(|declared| {
            crate::class_types::known_class_match(self.code.cache(function), declared, value)
        })
    }

    /// The same for a general callee, which is native code: any value, for
    /// an untyped by-value parameter. A pending named argument takes the
    /// general path.
    fn general_accepts(&self, index: usize) -> bool {
        let Some(details) = self.stack.top_details() else {
            return false;
        };
        let Some(callee) = details.callee.as_ref().filter(|_| details.named.is_none()) else {
            return false;
        };
        let intrinsic = matches!(callee.target, CallTarget::Intrinsic(_));
        callee.parameters.get(index).is_some_and(|parameter| {
            !intrinsic && !parameter.by_reference && !parameter.variadic && parameter.ty.is_none()
        })
    }

    /// `Call` of a prepared function, method or closure written in PHP whose
    /// arguments all arrived positionally and passed their checks: push the
    /// frame. `$this` and a closure's captures are in the window already.
    #[inline(always)]
    pub(super) fn enter_prepared(&mut self, position: Position, destination: Reg) -> Step {
        let Some(call) = self.stack.last_call().copied() else {
            return Step::Slow;
        };
        let function = match call.callee {
            PreparedCallee::Php(function) => function,
            PreparedCallee::Constructed => return self.finish_construction(position, destination),
            PreparedCallee::General => {
                return self.finish_direct_method(position, destination, call);
            }
        };
        let facts = self.code.facts(function);
        let sent = call.sent as usize;
        if call.flags != CallFlags::default()
            || sent < facts.required as usize
            || sent > facts.parameters as usize
            || facts.shape != FunctionShape::Plain
            || self.stack.depth() >= self.frame_limit
        {
            return Step::Slow;
        }
        let Some(call) = self.stack.pop_plain() else {
            return Step::Slow;
        };
        self.publish(position.ip);
        let mut frame = Frame::of_kind(function, call.kind, call.base(), Some(destination));
        if sent < facts.parameters as usize {
            frame = frame.with_passed(sent);
        }
        self.stack.push(frame);
        if call.called.is_some() {
            self.stack.current_extras_mut().called = call.called;
        }
        self.spend_work();
        Step::Switched(Position { frame, ip: 0 })
    }
}

impl Machine<'_> {
    /// `Call` after `new` of a class without a constructor: the object is the
    /// result, and the arguments, already evaluated, are dropped.
    fn finish_construction(&mut self, position: Position, destination: Reg) -> Step {
        let Some(call) = self.stack.pop_plain() else {
            return Step::Slow;
        };
        let object = self.registers.take(call.base() - 1);
        self.registers.truncate(call.start());
        self.registers
            .put(position.frame.base() + destination.index(), object);
        Step::Done(position.ip)
    }
}
