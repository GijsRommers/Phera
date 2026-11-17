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
//! nothing else could shadow it. Everything else answers [`Step::Slow`]
//! without having changed anything, and `Vm::execute` applies every rule.

use std::rc::Rc;

use phera_bytecode::{ClassBinding, NameId, Reg, SymbolId, Visibility};
use phera_value::{Object, Value};

use crate::calls::call_state::{CallTarget, Callee};
use crate::calls::execution_loop::{Machine, Position, Step};
use crate::classes::method_table::{FoundMethod, MethodTable};
use crate::classes::{Class, Invocation, MethodBody, MethodContext};
use crate::code_table::NameKey;

impl Machine<'_> {
    /// `InitMethodCall` of a public instance method written in PHP: reserve
    /// the method's window and record the call with its receiver.
    #[inline(always)]
    pub(super) fn prepare_method(
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
        self.prepare_found(found, Some(object), class.clone(), position.ip)
    }

    /// `InitStaticCall` of `self::`, `parent::`, `static::` or a named class,
    /// to a static method, or to an instance method of the running object
    /// through `self::` or `parent::`.
    #[inline(always)]
    pub(super) fn prepare_static(
        &mut self,
        position: Position,
        symbols: (SymbolId, SymbolId),
        binding: ClassBinding,
    ) -> Step {
        let (class_symbol, method_symbol) = symbols;
        let function = position.frame.function;
        let context = self.current_context().cloned();
        let written = &self.code.unit(function).symbols[class_symbol.index()].written;
        let (class, relative) = match (written.as_str(), &context) {
            ("self", Some(context)) => (context.scope.clone(), true),
            ("static", Some(context)) => (context.called.clone(), true),
            ("parent", Some(context)) => match &context.scope.parent {
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
        let scope_ok = self.may_call(&found);
        let receiver = match (&context, found.method.is_static, relative) {
            (_, true, _) => None,
            (Some(context), false, true) => context.receiver.clone(),
            _ => return Step::Slow,
        };
        if !scope_ok
            || (!found.method.is_static && receiver.is_none())
            || self.caller_has_other_private(key, found.method)
        {
            return Step::Slow;
        }
        let called = match (binding, &context) {
            (ClassBinding::Forward, Some(context)) => context.called.clone(),
            _ => class.clone(),
        };
        self.prepare_found(found, receiver, called, position.ip)
    }

    /// Record a prepared call of a found method written in PHP.
    fn prepare_found(
        &mut self,
        found: FoundMethod<'_>,
        receiver: Option<Rc<Object>>,
        called: Rc<Class>,
        ip: usize,
    ) -> Step {
        let method = found.method;
        if method.is_abstract || !matches!(method.body, MethodBody::User(_)) {
            return Step::Slow;
        }
        let Some(scope) = found.scope.upgrade() else {
            return Step::Slow;
        };
        let signature = method.signature(self.code);
        let callee = Callee {
            argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
            name: signature.name.clone(),
            parameters: signature.parameters.clone(),
            target: CallTarget::Method {
                method: method.clone(),
                context: MethodContext {
                    receiver: receiver.filter(|_| !method.is_static),
                    scope,
                    called,
                    invocation: Invocation::Method,
                },
            },
        };
        let Ok(size) = crate::calls::pending::window_registers(self.code, &callee) else {
            return Step::Slow;
        };
        crate::calls::pending::push_prepared(self.stack, self.registers, callee, size);
        Step::Done(ip)
    }

    /// Whether the running frame may call the method without the general
    /// visibility check: it is public, or private and called from inside
    /// the class that declares it. Protected methods take the general path.
    fn may_call(&self, found: &FoundMethod<'_>) -> bool {
        match found.method.visibility {
            Visibility::Public => true,
            Visibility::Private => self.current_context().is_some_and(|context| {
                found
                    .scope
                    .upgrade()
                    .is_some_and(|scope| Rc::ptr_eq(&scope, &context.scope))
            }),
            Visibility::Protected => false,
        }
    }

    /// The class a named class symbol was linked to, once some earlier
    /// operation of this file resolved it.
    fn linked_class(&self, position: Position, symbol: SymbolId) -> Option<Rc<Class>> {
        let cache = self.code.cache(position.frame.function);
        cache.classes.get(symbol.index())?.borrow().clone()
    }

    /// The class context of the running frame, if it is a method.
    fn current_context(&self) -> Option<&MethodContext> {
        let frame = self.stack.current()?;
        if !frame.has_extras() {
            return None;
        }
        self.stack.extras(self.stack.depth() - 1).method.as_ref()
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
        self.current_context().is_some_and(|context| {
            self.method_table(&context.scope)
                .get(key)
                .is_some_and(|found| {
                    found.method.visibility == Visibility::Private
                        && !Rc::ptr_eq(found.method, method)
                })
        })
    }
}
