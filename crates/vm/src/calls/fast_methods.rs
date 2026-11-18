//! Method calls that the interpreter loop prepares without the VM.
//!
//! ```php
//! $point->add(1, 2);       // InitMethodCall: method of the receiver's class
//! self::helper();          // InitStaticCall: method of the class scope
//! parent::__toString();    // InitStaticCall: method of the parent class
//! Point::origin();         // InitStaticCall: method of a named class
//! ```
//!
//! The method is found in the class's [`MethodTable`] by the interned name
//! the compiler wrote. Only the plain cases are taken here: a public method
//! (or a private one called from its own class), written in PHP, where
//! nothing else could shadow it. The call is one record and a window whose
//! first register holds `$this`. Everything else answers [`Step::Slow`]
//! without having changed anything, and `Vm::execute` applies every rule.

use std::rc::Rc;

use phera_bytecode::{ClassBinding, NameId, Reg, SymbolId, Visibility};
use phera_extension_api::{NativeMethod, ParamType};
use phera_value::{Object, Value};

use crate::calls::call_state::{CallTarget, Callee};
use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::pending::prepare_general;
use crate::calls::php_windows::push_php_call;
use crate::calls::prepared_call::{CallFlags, CallKind};
use crate::calls::window_context::WindowContext;
use crate::classes::method_table::{FoundMethod, MethodTable};
use crate::classes::{Class, ClassIndex, Invocation, MethodBody, MethodContext};
use crate::code_table::NameKey;

impl Machine<'_> {
    /// `InitMethodCall` of a public instance method written in PHP: reserve
    /// the method's window with the receiver in its first register.
    #[inline(always)]
    pub(super) fn prepare_method<const TRACK_GENERATORS: bool>(
        &mut self,
        position: Position,
        receiver: Reg,
        name: NameId,
    ) -> Step {
        let slot = position.frame.base() + receiver.index();
        let object = match self.registers.window(slot, 1) {
            [Some(Value::Object(object))] => object.clone(),
            _ => return Step::Slow,
        };
        let class = object.class.as_any().downcast_ref::<Class>();
        let Some(class) = class.and_then(|class| class.identity.upgrade()) else {
            return Step::Slow;
        };
        let key = self.code.unit_name_key(position.frame.function, name);
        let Some(found) = self.method_table(&class).get(key) else {
            return Step::Slow;
        };
        if !self.may_call(&found)
            || found.method.is_static
            || self.caller_has_other_private(key, found.method)
        {
            return Step::Slow;
        }
        if TRACK_GENERATORS
            && let MethodBody::Native(entry) = found.method.body
            && matches!(entry.handler, NativeMethod::Window(_))
        {
            return self.prepare_found_window(found, object, &class, position, receiver);
        }
        self.prepare_found(found, Some(object), None, position.ip)
    }

    /// The linked method table has already resolved this native window.
    #[inline(never)]
    fn prepare_found_window(
        &mut self,
        found: FoundMethod<'_>,
        receiver: Rc<Object>,
        called: &Rc<Class>,
        position: Position,
        receiver_register: Reg,
    ) -> Step {
        let MethodBody::Native(entry) = found.method.body else {
            return Step::Slow;
        };
        let Some(scope) = found.scope.upgrade() else {
            return Step::Slow;
        };
        let signature = found.method.signature(self.code);
        let slot = position.frame.base() + receiver_register.index();
        if receiver_register.index() >= self.code.function(position.frame.function).locals.len() {
            self.registers.take(slot);
        }
        prepare_general(
            self.stack,
            self.registers,
            Callee {
                argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
                name: signature.name.clone(),
                parameters: signature.parameters.clone(),
                target: CallTarget::Method {
                    method: found.method.clone(),
                    context: MethodContext {
                        receiver: Some(receiver),
                        scope,
                        called: called.clone(),
                        invocation: Invocation::Method,
                    },
                },
            },
        );
        if let [parameter] = entry.params
            && matches!(parameter.ty, ParamType::Mixed)
            && !parameter.by_ref
            && !parameter.variadic
            && let Some(call) = self.stack.last_call_mut()
        {
            call.flags.insert(CallFlags::WINDOW_ARGUMENT);
        }
        Step::Done(position.ip)
    }

    /// `InitStaticCall` of `self::`, `parent::`, `static::` or a named class,
    /// to a static method, or to an instance method of the running object
    /// through `self::`, `parent::` or `static::`.
    #[inline(always)]
    pub(super) fn prepare_static(
        &mut self,
        position: Position,
        symbols: (SymbolId, SymbolId),
        binding: ClassBinding,
    ) -> Step {
        let (class_symbol, method_symbol) = symbols;
        let function = position.frame.function;
        let context = self.current_context();
        let scope = context.and_then(|context| context.scope());
        let written = &self.code.unit(function).symbols[class_symbol.index()].written;
        let (class, relative) = match (written.as_str(), scope) {
            ("self", Some(scope)) => (scope.clone(), true),
            ("static", Some(_)) => match context.and_then(|context| context.called()) {
                Some(called) => (called, true),
                None => return Step::Slow,
            },
            ("parent", Some(scope)) => match &scope.parent {
                Some(parent) => (parent.clone(), true),
                None => return Step::Slow,
            },
            ("self" | "static" | "parent", None) => return Step::Slow,
            _ => match self.linked_class(position, class_symbol) {
                Some(class) => (class, false),
                None => return Step::Slow,
            },
        };
        let key = self.code.unit_symbol_key(function, method_symbol);
        let Some(found) = self.method_table(&class).get(key) else {
            return Step::Slow;
        };
        let receiver = match (found.method.is_static, relative) {
            (true, _) => None,
            (false, true) => context.and_then(|context| context.this()).cloned(),
            (false, false) => return Step::Slow,
        };
        if !self.may_call(&found)
            || (!found.method.is_static && receiver.is_none())
            || self.caller_has_other_private(key, found.method)
        {
            return Step::Slow;
        }
        let called = match (binding, context) {
            (ClassBinding::Forward, Some(context)) => context.called().map(|called| called.index),
            _ => Some(class.index),
        };
        self.prepare_found(found, receiver, called, position.ip)
    }

    /// Record a prepared call of a found method written in PHP, with
    /// `$this` in the window's first register. `called` is the class a
    /// static method was called on.
    fn prepare_found(
        &mut self,
        found: FoundMethod<'_>,
        receiver: Option<Rc<Object>>,
        called: Option<ClassIndex>,
        ip: usize,
    ) -> Step {
        let method = found.method;
        let MethodBody::User(function) = method.body else {
            return Step::Slow;
        };
        if method.is_abstract {
            return Step::Slow;
        }
        let this = receiver.filter(|_| !method.is_static).map(Value::Object);
        let has_this = this.is_some();
        let parts = (&mut *self.stack, &mut *self.registers);
        push_php_call(self.code, parts, function, CallKind::Method, [this]);
        let scope = self.code.facts(function).class;
        if let Some(call) = self.stack.last_call_mut()
            && !has_this
            && called != scope
        {
            call.called = called;
        }
        Step::Done(ip)
    }

    /// Whether the running frame may call the method without the general
    /// visibility check: it is public, or private and called from inside
    /// the class that declares it. Protected methods take the general path.
    fn may_call(&self, found: &FoundMethod<'_>) -> bool {
        match found.method.visibility {
            Visibility::Public => true,
            Visibility::Private => {
                let scope = self
                    .current_context()
                    .and_then(|context| context.scope_index());
                found
                    .scope
                    .upgrade()
                    .is_some_and(|declaring| Some(declaring.index) == scope)
            }
            Visibility::Protected => false,
        }
    }

    /// The class a named class symbol was linked to, once some earlier
    /// operation of this file resolved it.
    fn linked_class(&self, position: Position, symbol: SymbolId) -> Option<Rc<Class>> {
        let cache = self.code.cache(position.frame.function);
        cache.classes.get(symbol.index())?.borrow().clone()
    }

    /// The window of the running frame, if it is a method or closure.
    fn current_context(&self) -> Option<WindowContext<'_>> {
        let frame = self.stack.current()?;
        if frame.kind() == CallKind::Function {
            return None;
        }
        Some(WindowContext {
            kind: frame.kind(),
            function: frame.function,
            base: frame.base(),
            called: self.stack.extras(self.stack.depth() - 1).called,
            code: self.code,
            registers: self.registers,
            classes: self.classes,
        })
    }

    /// The class's method table, built on first use.
    fn method_table<'c>(&self, class: &'c Class) -> &'c MethodTable {
        class.method_table.get_or_init(|| {
            MethodTable::build(class, self.code, |method| {
                self.classes.get(&method.declaring)
            })
        })
    }

    /// Whether the running method's class has a different, private method
    /// of this name, which a call from inside that class could reach instead.
    fn caller_has_other_private(&self, key: NameKey, method: &Rc<crate::classes::Method>) -> bool {
        let Some(scope) = self.current_context().and_then(|context| context.scope()) else {
            return false;
        };
        self.method_table(scope).get(key).is_some_and(|found| {
            found.method.visibility == Visibility::Private && !Rc::ptr_eq(found.method, method)
        })
    }
}
