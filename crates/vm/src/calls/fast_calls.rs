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

use crate::calls::call_state::{ArgumentOrder, CallTarget, Callee, TypeChecks};
use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::frame::Frame;
use crate::classes::MethodContext;
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
        let base = self.registers.len();
        self.registers.grow_to(base + facts.registers as usize);
        for index in 0..count {
            self.registers.move_slot(from + index, base + index);
        }
        self.publish(caller.ip);
        let frame = Frame::new(function, base, Some(destination));
        self.stack.push(frame);
        if count < facts.parameters as usize {
            let arguments = &mut self.stack.current_extras_mut().arguments;
            arguments.supplied = Some(count);
            arguments.count = Some(count);
        }
        self.spend_work();
        Step::Switched(Position { frame, ip: 0 })
    }

    /// `Return` from a function or method frame that keeps nothing else in
    /// the side table, returns to PHP code, and whose value passes its
    /// return type's bit test: write the value into the caller's register
    /// and empty the callee's.
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
        if (frame.has_extras() && !self.stack.extras(depth - 1).holds_only_call_context())
            || self.stack.depth() <= self.stop + 1
            || self.code.local(frame.function).is_none()
            || self.stack.has_calls_from(frame.base())
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
        let value = match source {
            Some(source) => self.registers.take(frame.base() + source.index()),
            None => Value::Null,
        };
        self.stack.pop();
        self.registers.truncate(frame.base());
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
        let Some(call) = self.stack.preparing.last() else {
            return Step::Slow;
        };
        let index = call.window.supplied;
        if call.window.named.is_some()
            || index >= call.window.parameters
            || matches!(call.callee.target, CallTarget::Intrinsic(_))
        {
            return Step::Slow;
        }
        let parameter = &call.callee.parameters[index];
        if parameter.by_reference || parameter.variadic {
            return Step::Slow;
        }
        let value = match self
            .registers
            .window(position.frame.base() + source.index(), 1)
        {
            [Some(value)] => value.deref().clone(),
            _ => return Step::Slow,
        };
        let accepted = match php_function(&call.callee) {
            Some(function) => self.accepts_argument(function, index, &value),
            None => parameter.ty.is_none(),
        };
        if !accepted {
            return Step::Slow;
        }
        let Some(call) = self.stack.preparing.last_mut() else {
            return Step::Slow;
        };
        self.registers.put(call.window.base + index, value);
        call.window.supplied = index + 1;
        call.window.argument_count = call.window.argument_count.max(index + 1);
        Step::Done(position.ip)
    }

    /// Whether parameter `index` of `function` takes `value` as it is: no
    /// declared type, a built-in type the value has, or a class type the
    /// unit's cache knows the value's class satisfies.
    fn accepts_argument(&self, function: FunctionIndex, index: usize, value: &Value) -> bool {
        let facts = self.code.facts(function);
        if !facts.is_typed(index) {
            return index < 64;
        }
        if facts.parameter_masks[index].intersects(crate::types::own_type(value)) {
            return true;
        }
        let declared = self.code.function(function).parameters[index].ty;
        declared.is_some_and(|declared| {
            crate::class_types::known_class_match(self.code.cache(function), declared, value)
        })
    }

    /// `Call` of a prepared function or method written in PHP whose
    /// arguments all arrived positionally and passed their checks: push the
    /// frame, with the method's class context beside it.
    #[inline(always)]
    pub(super) fn enter_prepared(&mut self, position: Position, destination: Reg) -> Step {
        let Some(call) = self.stack.preparing.last() else {
            return Step::Slow;
        };
        let Some(function) = php_function(&call.callee) else {
            return Step::Slow;
        };
        let facts = self.code.facts(function);
        let window = &call.window;
        if window.checks != TypeChecks::Complete
            || window.order != ArgumentOrder::Positional
            || window.supplied < facts.required as usize
            || window.supplied > facts.parameters as usize
            || facts.shape != FunctionShape::Plain
            || self.stack.depth() >= self.frame_limit
        {
            return Step::Slow;
        }
        let Some(call) = self.stack.preparing.pop() else {
            return Step::Slow;
        };
        let (method, closure) = match call.callee.target {
            CallTarget::Method { context, .. } => (Some(context), call.owner),
            CallTarget::Closure(object) => {
                let context = self.copy_captures(&object, function, call.window.base);
                (context, Some(object))
            }
            _ => (None, call.owner),
        };
        self.publish(position.ip);
        let frame = Frame::new(function, call.window.base, Some(destination));
        self.stack.push(frame);
        let supplied = call.window.supplied;
        if method.is_some() || closure.is_some() || supplied < facts.parameters as usize {
            let extras = self.stack.current_extras_mut();
            extras.method = method;
            extras.closure = closure;
            if supplied < facts.parameters as usize {
                extras.arguments.supplied = Some(supplied);
                extras.arguments.count = Some(call.window.argument_count);
            }
        }
        self.spend_work();
        Step::Switched(Position { frame, ip: 0 })
    }
}

impl Machine<'_> {
    /// Copy a closure's captured values into the registers after its
    /// parameters, and return the class context it was created in.
    fn copy_captures(
        &mut self,
        object: &std::rc::Rc<phera_value::Object>,
        function: FunctionIndex,
        base: usize,
    ) -> Option<MethodContext> {
        let native = object.native.borrow();
        let state = native
            .as_ref()?
            .downcast_ref::<crate::closures::ClosureState>()?;
        let offset = base + self.code.function(function).parameters.len();
        for (index, value) in state.captures.iter().enumerate() {
            if let Some(value) = value {
                self.registers.put(offset + index, value.clone());
            }
        }
        state.context.clone()
    }
}

/// The PHP function a prepared call enters, when its target is PHP code: a
/// function, a method written in PHP, or a closure over a PHP function.
fn php_function(callee: &Callee) -> Option<FunctionIndex> {
    match &callee.target {
        CallTarget::Closure(object) => object
            .native
            .borrow()
            .as_ref()?
            .downcast_ref::<crate::closures::ClosureState>()?
            .callee
            .user_function(),
        _ => callee.user_function(),
    }
}
