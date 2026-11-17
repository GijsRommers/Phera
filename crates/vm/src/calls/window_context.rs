//! What a method's or closure's window holds below its first parameter.
//!
//! ```php
//! class Counter {
//!     public function add(int $step) { $this->count += $step; }
//! }
//! $counter->add(2);   // window: [ $counter | 2, ... ]
//! ```
//!
//! A method's window starts with `$this`, one register below the first
//! parameter, empty for a static method. A closure's starts with the
//! closure object and its bound `$this`. From these and the function, the
//! class scope (`self::`) and the called class (`static::`) follow, so a
//! call stores neither: the scope of a method is the class that declares
//! it, of a closure the scope it was created or bound in; the called class
//! is the class of `$this`, or for a static method the class it was called
//! on, which only needs storing when it is not the scope.

use std::rc::Rc;

use phera_value::{Object, Value};

use crate::calls::prepared_call::CallKind;
use crate::calls::registers::Registers;
use crate::classes::{Class, ClassIndex, ClassTable, Invocation, MethodContext};
use crate::code_table::{CodeTable, FunctionIndex};

/// A window of a running or prepared call, with what is needed to read it.
#[derive(Clone, Copy)]
pub(crate) struct WindowContext<'a> {
    pub(crate) kind: CallKind,
    pub(crate) function: FunctionIndex,
    /// The register of the first parameter.
    pub(crate) base: usize,
    /// The called class, when it is stored because it cannot be derived.
    pub(crate) called: Option<ClassIndex>,
    pub(crate) code: &'a CodeTable,
    pub(crate) registers: &'a Registers,
    pub(crate) classes: &'a ClassTable,
}

impl<'a> WindowContext<'a> {
    /// `$this`, for a method or closure that has one.
    pub(crate) fn this(&self) -> Option<&'a Rc<Object>> {
        match self.kind {
            CallKind::Function => None,
            _ => object_in(self.registers, self.base - 1),
        }
    }

    /// The closure object, for a closure.
    pub(crate) fn closure(&self) -> Option<&'a Rc<Object>> {
        match self.kind {
            CallKind::Closure => object_in(self.registers, self.base - 2),
            _ => None,
        }
    }

    /// The number of the class scope.
    pub(crate) fn scope_index(&self) -> Option<ClassIndex> {
        match self.kind {
            CallKind::Method | CallKind::Constructor => self.code.facts(self.function).class,
            CallKind::Closure => closure_context(self.closure()?, |context| context.scope.index),
            CallKind::Function => None,
        }
    }

    /// The class scope (`self::`).
    pub(crate) fn scope(&self) -> Option<&'a Rc<Class>> {
        Some(self.classes.class(self.scope_index()?))
    }

    /// The called class (`static::`): the class of `$this`, else the class a
    /// static method was called on, else the class scope.
    pub(crate) fn called(&self) -> Option<Rc<Class>> {
        if let Some(this) = self.this() {
            return self.classes.for_object(this);
        }
        if let Some(called) = self.called {
            return Some(self.classes.class(called).clone());
        }
        match self.kind {
            CallKind::Closure => closure_context(self.closure()?, |context| context.called.clone()),
            _ => self.scope().cloned(),
        }
    }

    /// The whole class context, for the general path.
    pub(crate) fn method(&self) -> Option<MethodContext> {
        let scope = self.scope()?.clone();
        Some(MethodContext {
            receiver: self.this().cloned(),
            called: self.called().unwrap_or_else(|| scope.clone()),
            scope,
            invocation: match self.kind {
                CallKind::Constructor => Invocation::Constructor,
                _ => Invocation::Method,
            },
        })
    }
}

/// The object in register `slot`, if it holds one.
fn object_in(registers: &Registers, slot: usize) -> Option<&Rc<Object>> {
    match registers.window(slot, 1) {
        [Some(Value::Object(object))] => Some(object),
        _ => None,
    }
}

/// Read the class context a closure object was created or bound with.
fn closure_context<T>(closure: &Object, read: impl FnOnce(&MethodContext) -> T) -> Option<T> {
    let native = closure.native.borrow();
    let state = native
        .as_ref()?
        .downcast_ref::<crate::closures::ClosureState>()?;
    state.context.as_ref().map(read)
}
