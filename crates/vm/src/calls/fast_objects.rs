//! `new`, closure calls and `$this` in the interpreter loop.
//!
//! ```php
//! $point = new Point(1, 2);   // New: allocate, prepare the constructor
//! $total += $scale($value);   // InitDynamicCall of a closure
//! return $this;               // FetchThis
//! ```
//!
//! `new` of a class whose property defaults are all literals allocates the
//! object from the class's template and prepares the public constructor
//! written in PHP, if there is one, as a method call with the new object as
//! `$this`. A closure call puts the closure object, its bound `$this` and
//! its captured values into the window. Anything else (an abstract class,
//! defaults that run code, a native constructor, a closure made from a
//! callable) answers [`Step::Slow`] without having changed anything.

use phera_bytecode::{ClassKind, Instruction, Reg, SymbolId, Visibility};
use phera_value::Value;

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::php_windows::{push_closure_call, push_constructed, push_php_call};
use crate::calls::prepared_call::CallKind;
use crate::classes::MethodBody;
use crate::classes::class::ClassOrigin;
use crate::closures::{ClosureOrigin, ClosureState};

impl Machine<'_> {
    /// `New` of a class this file has linked, with literal defaults and no
    /// constructor or a public one written in PHP.
    #[inline(always)]
    pub(super) fn new_object<const GUARDED: bool>(
        &mut self,
        position: Position,
        symbol: SymbolId,
    ) -> Step {
        let key = self.code.unit_symbol_key(position.frame.function, symbol);
        let Some(class) = self.classes.by_key(key).cloned() else {
            return Step::Slow;
        };
        let Some(template) = class.property_template.as_ref() else {
            return Step::Slow;
        };
        // An object with a destructor makes the plain loop unsafe: the
        // general handler creates it, and the next batch is guarded.
        if class.origin != ClassOrigin::User
            || class.kind != ClassKind::Class
            || class.is_abstract
            || class.is_throwable
            || (!GUARDED
                && class
                    .magic
                    .as_ref()
                    .is_some_and(|magic| magic.destruct.is_some()))
        {
            return Step::Slow;
        }
        let constructor = match &class.constructor {
            None => None,
            Some(method) => match method.body {
                MethodBody::User(function)
                    if method.visibility == Visibility::Public && !method.is_abstract =>
                {
                    Some(function)
                }
                _ => return Step::Slow,
            },
        };
        let template = template.clone();
        let object = self.classes.allocate_with_properties(class, template);
        if constructor.is_none()
            && !GUARDED
            && let Some(Instruction::Call { dst, .. }) = self
                .code
                .instructions(position.frame.function)
                .get(position.ip)
        {
            let slots = self.registers.tail_mut(position.frame.base());
            if let Some(slot) = slots.get_mut(dst.index()) {
                *slot = Some(Value::Object(object));
                return Step::Done(position.ip + 1);
            }
        }
        let parts = (&mut *self.stack, &mut *self.registers);
        match constructor {
            Some(function) => {
                let this = Some(Value::Object(object));
                push_php_call(self.code, parts, function, CallKind::Constructor, [this]);
            }
            None => push_constructed(parts, object),
        }
        Step::Done(position.ip)
    }

    /// `InitDynamicCall` of a closure written in PHP: the closure object,
    /// its bound `$this` and its captured values go into the window.
    #[inline(always)]
    pub(super) fn prepare_closure(&mut self, position: Position, callee: Reg) -> Step {
        let slot = position.frame.base() + callee.index();
        let object = match self.registers.window(slot, 1) {
            [Some(Value::Object(object))] => object.clone(),
            _ => return Step::Slow,
        };
        let lexical = object
            .native
            .borrow()
            .as_ref()
            .and_then(|state| state.downcast_ref::<ClosureState>())
            .is_some_and(|state| state.origin == ClosureOrigin::Lexical);
        let parts = (&mut *self.stack, &mut *self.registers);
        if !lexical || !push_closure_call(self.code, parts, &object) {
            return Step::Slow;
        }
        Step::Done(position.ip)
    }

    /// `FetchThis` and `ThisOrNull` in a frame that has `$this`. Without
    /// one, `FetchThis` throws, which the general path does.
    #[inline(always)]
    pub(super) fn fetch_this(&mut self, position: Position, destination: Reg) -> Step {
        let frame = position.frame;
        if frame.kind() == CallKind::Function {
            return Step::Slow;
        }
        let this = match self.registers.window(frame.base() - 1, 1) {
            [Some(Value::Object(object))] => Value::Object(object.clone()),
            _ => return Step::Slow,
        };
        self.registers.put(frame.base() + destination.index(), this);
        Step::Done(position.ip)
    }
}
