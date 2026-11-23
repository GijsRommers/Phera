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

use phera_bytecode::{FunctionId, Instruction, Reg};
use phera_value::Value;

use crate::calls::call_state::CallTarget;
use crate::calls::discarded_results::{frees_on_drop, result_discarded};
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
        let base = self.registers.len();
        let size = facts.registers as usize;
        if from + count > base || count > size {
            return Step::Slow;
        }
        let arguments = self.registers.window(from, count);
        let mut typed = facts.typed_parameters();
        for (argument, mask) in arguments.iter().zip(facts.parameter_masks.iter()) {
            let Some(value) = argument else {
                return Step::Slow;
            };
            if value.is_reference()
                || (typed & 1 != 0 && !mask.intersects(crate::types::own_type(value)))
            {
                return Step::Slow;
            }
            typed >>= 1;
        }
        self.registers.grow_to(base + size);
        for index in 0..count {
            self.registers.move_slot(from + index, base + index);
        }
        self.publish(caller.ip);
        let mut frame = Frame::new(function, base, Some(destination));
        if count < facts.parameters as usize {
            frame = frame.with_passed(count);
        }
        self.stack.push(frame);

        if self.spend_work() {
            return Step::Yield;
        }
        Step::Switched(Position { frame, ip: 0 })
    }

    /// `Return` from a function or method frame that keeps nothing else in
    /// the side table, returns to PHP code, and whose value passes its
    /// return type's bit test: write the value (for a constructor, the new
    /// object) into the caller's register and empty the callee's.
    #[inline(always)]
    pub(super) fn leave_plain<const GUARDED: bool>(&mut self, source: Option<Reg>) -> Step {
        let Some(frame) = self.stack.current().copied() else {
            return Step::Slow;
        };
        let Some(destination) = frame.destination() else {
            return if frame.discards_result() {
                self.leave_discarded(frame, source)
            } else if !GUARDED && frame.returns_to_native_loop() {
                self.leave_to_native_loop(frame, source)
            } else if !GUARDED && self.stack.depth() == self.stop + 1 {
                self.leave_to_native(frame, source)
            } else {
                Step::Slow
            };
        };
        let facts = self.code.facts(frame.function);
        let depth = self.stack.depth();
        let kind = frame.kind();
        let start = frame.base() - kind.prefix();
        if depth <= self.stop + 1
            || facts.local.is_none()
            || (frame.has_extras() && !self.stack.extras(depth - 1).plain_return_can_release())
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
            && !return_fits(mask, value)
            && !self
                .code
                .function(frame.function)
                .return_type
                .is_some_and(|declared| self.class_type_matches(frame.function, declared, value))
        {
            return Step::Slow;
        }
        let value = match (kind, source) {
            (CallKind::Constructor, _) => self.registers.take(frame.base() - 1),
            (_, Some(source)) => self.registers.take(frame.base() + source.index()),
            (_, None) => Value::Null,
        };
        // Where the order of releases can be seen, the slower way out.
        if GUARDED
            && ((kind != CallKind::Constructor && frees_on_drop(&value))
                || (start != frame.base()
                    && self.registers.holds_last_reference(start, frame.base())))
        {
            return self.leave_in_order(frame, caller, (source, destination), value);
        }
        self.stack.pop_for_return();
        self.registers.release_return_window(start);
        self.registers
            .put(caller.base() + destination.index(), value);
        Step::Switched(Position {
            frame: caller,
            ip: caller.ip(),
        })
    }
}

impl Machine<'_> {
    /// A plain return whose releases PHP orders visibly: an unused
    /// temporary result is freed first (see `discarded_results`), then the
    /// variables, then `$this` and the closure.
    #[cold]
    #[inline(never)]
    fn leave_in_order(
        &mut self,
        frame: Frame,
        caller: Frame,
        (source, destination): (Option<Reg>, Reg),
        value: Value,
    ) -> Step {
        let value = if self.discards_result(frame, caller, source, destination) {
            drop(value);
            None
        } else {
            Some(value)
        };
        self.stack.pop_for_return();
        self.registers.release_frame(frame.start(), frame.base());
        if let Some(value) = value {
            self.registers
                .put(caller.base() + destination.index(), value);
        }
        Step::Switched(Position {
            frame: caller,
            ip: caller.ip(),
        })
    }

    /// A plain return of the function native code called (a callback of
    /// `array_map`, `usort`, a `Closure` called from native code), as the
    /// VM makes it (`Vm::finish_return_value` at the boundary): the frame
    /// goes, and the value is handed to the batch loop, which gives it back
    /// to the native caller as `BatchExit::Returned`. Generators, includes,
    /// functions returning by reference, frames with a pending call or side
    /// state to release, references, and a value the declared type does not
    /// take as it is stay with the VM.
    #[cold]
    #[inline(never)]
    fn leave_to_native(&mut self, frame: Frame, source: Option<Reg>) -> Step {
        let facts = self.code.facts(frame.function);
        let depth = self.stack.depth();
        let function = self.code.function(frame.function);
        if facts.local.is_none()
            || frame.returns_to_native_loop()
            || function.is_generator
            || function.returns_reference
            || (frame.has_extras() && !self.stack.extras(depth - 1).plain_return_can_release())
            || self.stack.has_calls_from(frame.start())
        {
            return Step::Slow;
        }
        let kind = frame.kind();
        let value = match (kind, source) {
            (CallKind::Constructor, _) => match self.registers.window(frame.base() - 1, 1) {
                [Some(value)] => value,
                _ => return Step::Slow,
            },
            (_, Some(source)) => match self.registers.window(frame.base() + source.index(), 1) {
                [Some(value)] if !value.is_reference() => value,
                _ => return Step::Slow,
            },
            (_, None) => &Value::Null,
        };
        if kind != CallKind::Constructor
            && facts
                .return_mask
                .is_some_and(|mask| !return_fits(mask, value))
        {
            return Step::Slow;
        }
        let value = value.clone();
        self.stack.pop_for_return();
        self.registers.release_frame(frame.start(), frame.base());
        self.stack.handed_back = Some(super::execution_loop::HandedBack::Native(value));
        Step::Yield
    }

    /// A plain return of a callback of a native loop (`array_map`'s
    /// closure), as the VM makes it: the frame goes, and the value goes to
    /// the batch loop, which hands it to the built-in's next step
    /// (`calls::native_loops`). The cases `leave_to_native` leaves to the
    /// VM stay with it here too.
    #[cold]
    #[inline(never)]
    fn leave_to_native_loop(&mut self, frame: Frame, source: Option<Reg>) -> Step {
        let facts = self.code.facts(frame.function);
        let depth = self.stack.depth();
        let function = self.code.function(frame.function);
        if facts.local.is_none()
            || function.is_generator
            || function.returns_reference
            || (frame.has_extras() && !self.stack.extras(depth - 1).plain_return_can_release())
            || self.stack.has_calls_from(frame.start())
        {
            return Step::Slow;
        }
        let value = match source {
            Some(source) => match self.registers.window(frame.base() + source.index(), 1) {
                [Some(value)] if !value.is_reference() => value,
                _ => return Step::Slow,
            },
            None => &Value::Null,
        };
        if facts
            .return_mask
            .is_some_and(|mask| !return_fits(mask, value))
        {
            return Step::Slow;
        }
        let value = value.clone();
        self.stack.pop_for_return();
        self.registers.release_frame(frame.start(), frame.base());
        self.stack.handed_back = Some(super::execution_loop::HandedBack::NativeLoop(value));
        Step::Yield
    }

    /// A setter with no suspended caller action returns without publishing a result.
    #[inline(never)]
    fn leave_discarded(&mut self, frame: Frame, source: Option<Reg>) -> Step {
        if self.stack.depth() <= self.stop + 1
            || frame.has_extras()
            || self.stack.has_calls_from(frame.start())
        {
            return Step::Slow;
        }
        let value = match source {
            None => &Value::Null,
            Some(source) => match self.registers.window(frame.base() + source.index(), 1) {
                [Some(value)] if !value.is_reference() => value,
                _ => return Step::Slow,
            },
        };
        if self
            .code
            .facts(frame.function)
            .return_mask
            .is_some_and(|mask| !return_fits(mask, value))
        {
            return Step::Slow;
        }
        let Some(caller) = self.stack.caller().copied() else {
            return Step::Slow;
        };
        self.stack.pop_for_return();
        self.registers.release_return_window(frame.start());
        Step::Switched(Position {
            frame: caller,
            ip: caller.ip(),
        })
    }

    /// Whether the caller throws away the temporary that `frame` returns.
    #[inline(never)]
    fn discards_result(
        &self,
        frame: Frame,
        caller: Frame,
        source: Option<Reg>,
        destination: Reg,
    ) -> bool {
        matches!(
            frame.kind(),
            CallKind::Function | CallKind::Method | CallKind::Closure
        ) && source
            .is_some_and(|source| source.index() >= self.code.function(frame.function).locals.len())
            && result_discarded(
                self.code.instructions(caller.function),
                caller.ip(),
                destination,
            )
    }
}

impl crate::Vm {
    /// A simple instruction that fell back only because the register it
    /// writes holds an array or an object: do it here, with the old value
    /// taken out and dropped (see `crate::fast`). Kept out of the machine
    /// loop, whose code a call here would slow down even where unused.
    #[inline(never)]
    pub(crate) fn release_and_store(&mut self, instruction: Instruction) -> bool {
        let Some(frame) = self.call_stack.current().copied() else {
            return false;
        };
        let slots = self.registers.tail_mut(frame.base());
        match crate::fast::releasing_step(instruction, slots) {
            Some(released) => {
                drop(released);
                true
            }
            None => false,
        }
    }
}

impl Machine<'_> {
    /// `SendValue` or `SendLocal` to a by-value parameter whose declared
    /// type accepts the value as it is: write it into the window.
    #[inline(always)]
    pub(super) fn send(&mut self, position: Position, source: Reg) -> Step {
        let value = match self
            .registers
            .window(position.frame.base() + source.index(), 1)
        {
            [Some(value)] => value.deref().clone(),
            _ => return Step::Slow,
        };
        self.send_owned(position, value).unwrap_or(Step::Slow)
    }

    /// Sends `value` as [`Self::send`] does, or gives it back untouched
    /// when the call takes the general path.
    #[inline(always)]
    pub(super) fn send_owned(
        &mut self,
        position: Position,
        mut value: Value,
    ) -> Result<Step, Value> {
        let Some(call) = self.stack.last_call().copied() else {
            return Err(value);
        };
        let index = call.sent as usize;
        let accepted = match call.callee {
            PreparedCallee::Php(function) => {
                !call.flags.contains(CallFlags::DETAILS)
                    && (self.accepts_argument(function, index, &value)
                        || self.accepts_on_second_look(function, index, &mut value))
            }
            // One out-of-line arm for both, so that the match keeps the
            // shape the loop was laid out with.
            PreparedCallee::General | PreparedCallee::Native(_) => {
                self.other_accepts(call.callee, index, &value)
            }
            PreparedCallee::Constructed => false,
        };
        if !accepted {
            return Err(value);
        }
        let Some(call) = self.stack.last_call_mut() else {
            return Err(value);
        };
        self.registers.put(call.base() + index, value);
        call.mark_sent(index, true);
        Ok(Step::Done(position.ip))
    }

    /// The second look at an argument the first test refused, out of line:
    /// an integer sent to a `float` parameter becomes that float, the one
    /// conversion PHP makes in strict mode too, so that `new Vec($i, 0)`
    /// for `float $x` stays in the loop; an object passes a class type its
    /// class satisfies. `false`, with `value` as it was, for anything else.
    #[cold]
    #[inline(never)]
    pub(super) fn accepts_on_second_look(
        &self,
        function: FunctionIndex,
        index: usize,
        value: &mut Value,
    ) -> bool {
        let facts = self.code.facts(function);
        if let Value::Object(_) = value {
            // An object of a class the declared type allows, that the
            // unit's one-entry cache did not remember.
            return facts.takes_value(index)
                && self
                    .code
                    .function(function)
                    .parameters
                    .get(index)
                    .and_then(|parameter| parameter.ty)
                    .is_some_and(|declared| self.declared_class_match(function, declared, value));
        }
        let Value::Int(integer) = *value else {
            return false;
        };
        let widens = facts.takes_value(index)
            && facts
                .parameter_masks
                .get(index)
                .is_some_and(|mask| mask.intersects(phera_bytecode::TypeMask::FLOAT));
        if widens {
            *value = Value::Float(integer as f64);
        }
        widens
    }

    /// Whether parameter `index` of `function` takes `value` as it is, by
    /// value: no declared type, a built-in type the value has, or a class
    /// type the unit's cache knows the value's class satisfies.
    #[inline(always)]
    pub(super) fn accepts_argument(
        &self,
        function: FunctionIndex,
        index: usize,
        value: &Value,
    ) -> bool {
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

    /// Whether `value` is an object of a class that the class type
    /// `declared` of `function` allows: the class the unit's cache last
    /// matched, or else the declared class found by its name key, with
    /// the object's linked ancestry walked by pointer.
    #[inline(always)]
    fn class_type_matches(
        &self,
        function: FunctionIndex,
        declared: phera_bytecode::TypeId,
        value: &Value,
    ) -> bool {
        crate::class_types::known_class_match(self.code.cache(function), declared, value)
            || self.declared_class_match(function, declared, value)
    }

    /// The second half of [`Machine::class_type_matches`]: a union of
    /// absolute class names (`Type`, `A|B`, `?Model`). `self`, `static`,
    /// `parent`, intersections and undeclared classes are left to the VM.
    #[cold]
    #[inline(never)]
    fn declared_class_match(
        &self,
        function: FunctionIndex,
        declared: phera_bytecode::TypeId,
        value: &Value,
    ) -> bool {
        let Value::Object(object) = value else {
            return false;
        };
        let Some(class) = object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
        else {
            return false;
        };
        let unit = self.code.unit(function);
        let Some(ty) = unit.types.get(declared.index()) else {
            return false;
        };
        if ty.intersection {
            return false;
        }
        let matches = ty.classes.iter().any(|name| {
            let relative = unit
                .names
                .get(name.index())
                .is_none_or(|written| matches!(written.as_str(), "self" | "static" | "parent"));
            !relative
                && self
                    .classes
                    .by_key(self.code.unit_name_key(function, *name))
                    .is_some_and(|target| class.derives_from(target) == Some(true))
        });
        // Remember it, as the VM's check does, so that the next value of
        // this class is the cache's first test.
        let cache = self.code.cache(function);
        if matches
            && cache.type_scopes.get(declared.index())
                == Some(&crate::types::DeclarationScope::Absolute)
            && let Some(Ok(mut slot)) = cache
                .class_types
                .get(declared.index())
                .map(|slot| slot.try_borrow_mut())
        {
            *slot = Some(crate::unit_caches::ClassTypeEntry {
                class: object.class.clone(),
                matches,
            });
        }
        matches
    }

    /// The same for a general callee, which is native code: any value, for
    /// an untyped by-value parameter. A pending named argument takes the
    /// general path.
    #[inline(always)]
    fn general_accepts(&self, index: usize, value: &Value) -> bool {
        let Some(details) = self.stack.top_details() else {
            return false;
        };
        let Some(callee) = details.callee.as_ref().filter(|_| details.named.is_none()) else {
            return false;
        };
        // `spl_object_id($object)`: an object needs no coercion, and the
        // batch loop answers the call (`Vm::object_id_call`).
        if let CallTarget::Intrinsic(crate::intrinsics::Intrinsic::Class(
            crate::class_intrinsics::ClassFunction::ObjectId,
        )) = callee.target
        {
            return index == 0 && matches!(value, Value::Object(_));
        }
        let intrinsic = matches!(callee.target, CallTarget::Intrinsic(_));
        callee.parameters.get(index).is_some_and(|parameter| {
            !intrinsic && !parameter.by_reference && !parameter.variadic && parameter.ty.is_none()
        })
    }

    /// [`Machine::general_accepts`] for a described callee; for a lean
    /// native call, [`Machine::native_accepts`]. Inline, so that a
    /// described callee costs what it did before lean records existed.
    #[inline(always)]
    pub(super) fn other_accepts(
        &self,
        callee: PreparedCallee,
        index: usize,
        value: &Value,
    ) -> bool {
        match callee {
            PreparedCallee::Native(id) => self.native_accepts(id, index),
            _ => self.general_accepts(index, value),
        }
    }

    /// For a lean native call: a by-value parameter that is not variadic
    /// (its type is the function's to check, at the call).
    #[inline(never)]
    fn native_accepts(&self, id: phera_extension_api::NativeId, index: usize) -> bool {
        self.registry
            .function_at(id)
            .and_then(|entry| entry.params.get(index))
            .is_some_and(|parameter| !parameter.by_ref && !parameter.variadic)
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
            // A lean native record is not a direct method, so it answers
            // `Step::Slow` there, and the batch loop runs it
            // (`Vm::call_lean_native`).
            PreparedCallee::General | PreparedCallee::Native(_) => {
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
        if self.spend_work() {
            return Step::Yield;
        }
        Step::Switched(Position { frame, ip: 0 })
    }
}

impl Machine<'_> {
    /// `Call` after `new` of a class without a constructor: the object is the
    /// result, and the arguments, already evaluated, are dropped.
    #[inline(always)]
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

/// Whether a returned value passes the bit test of the declared return
/// type: `return;` of a `void` function returns null.
fn return_fits(mask: phera_bytecode::TypeMask, value: &Value) -> bool {
    mask.intersects(crate::types::own_type(value))
        || (mask.intersects(phera_bytecode::TypeMask::VOID) && value.is_null())
}
