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
//! (or a private one called from its own class, or a protected one from a
//! related class), written in PHP, where
//! nothing else could shadow it. The call is one record and a window whose
//! first register holds `$this`. Everything else answers [`Step::Slow`]
//! without having changed anything, and `Vm::execute` applies every rule.

use std::rc::Rc;

use phera_bytecode::{ClassBinding, NameId, Reg, SymbolId, Visibility};
use phera_value::{Object, Value};

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::php_windows::push_php_call;
use crate::calls::prepared_call::CallKind;
use crate::calls::window_context::WindowContext;
use crate::classes::method_table::FoundMethod;
use crate::classes::{Class, ClassIndex, MethodBody};
use crate::code_table::{FunctionIndex, NameKey};

impl Machine<'_> {
    /// `InitMethodCall` of a public instance method written in PHP: reserve
    /// the method's window with the receiver in its first register.
    #[inline(always)]
    pub(super) fn prepare_method(
        &mut self,
        position: Position,
        receiver: Reg,
        name: NameId,
    ) -> Step {
        let slot = position.frame.base() + receiver.index();
        let object = match self.registers.window(slot, 1) {
            [Some(Value::Object(object))] => object,
            _ => return Step::Slow,
        };
        // The receiver owns its class through this lookup.
        let Some(class) = object.class.as_any().downcast_ref::<Class>() else {
            return Step::Slow;
        };
        let key = self.code.unit_name_key(position.frame.function, name);
        let Some(found) = self.classes.method_table(class).get(key) else {
            return Step::Slow;
        };
        let method = found.method;
        if method.is_abstract || method.is_static {
            return Step::Slow;
        }
        match method.body {
            MethodBody::User(function) => {
                let scope = self.running_scope();
                if !may_call(&found, scope) || self.shadowed_by_private(scope, key, method) {
                    return Step::Slow;
                }
                let receiver = Some(object.clone());
                self.prepare_php_method(function, receiver, None, position.ip)
            }
            // `Closure::__invoke` and the methods of a generator have their
            // own preparation in the VM.
            MethodBody::Native(_) if class.name == "Closure" || class.name == "Generator" => {
                Step::Slow
            }
            MethodBody::Native(entry) => {
                let Some((context, method)) =
                    self.native_class_method((object, class), &found, key)
                else {
                    return Step::Slow;
                };
                match entry.direct {
                    Some(_) => self.prepare_direct_method(position, context, method, entry),
                    None => self.prepare_native_method(position, context, method),
                }
            }
            _ => Step::Slow,
        }
    }

    /// The context and method for a call of the public method of a native
    /// class on `object`, as the VM prepares it. Out of line and cold, so
    /// that the PHP method path above compiles as it did before native
    /// methods were prepared here. `Closure::__invoke` and the methods of a
    /// generator have their own preparation in the VM.
    #[cold]
    #[inline(never)]
    fn native_class_method(
        &self,
        (object, class): (&Rc<Object>, &Class),
        found: &FoundMethod<'_>,
        key: NameKey,
    ) -> Option<(crate::classes::MethodContext, Rc<crate::classes::Method>)> {
        let method = found.method;
        if method.visibility != Visibility::Public
            || class.name == "Closure"
            || class.name == "Generator"
            || (self.classes.has_lazy_objects && object.lazy().is_some())
            || self.shadowed_by_private(self.running_scope(), key, method)
        {
            return None;
        }
        let context = crate::classes::MethodContext {
            receiver: Some(object.clone()),
            scope: found.scope.upgrade()?,
            called: class.identity.upgrade()?,
            invocation: crate::classes::Invocation::Method,
        };
        Some((context, method.clone()))
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
        let Some(found) = self.classes.method_table(&class).get(key) else {
            return Step::Slow;
        };
        let receiver = match (found.method.is_static, relative) {
            (true, _) => None,
            (false, true) => context.and_then(|context| context.this()).cloned(),
            (false, false) => return Step::Slow,
        };
        if !may_call(&found, scope)
            || (!found.method.is_static && receiver.is_none())
            || self.shadowed_by_private(scope, key, found.method)
        {
            return Step::Slow;
        }
        // A call with `$this` takes its called class from `$this`; only a
        // static call needs it worked out here.
        let called = match (receiver.is_some(), binding, context) {
            (true, _, _) => None,
            (false, ClassBinding::Forward, Some(context)) => context
                .called_index()
                .or_else(|| context.called().map(|called| called.index)),
            (false, _, _) => Some(class.index),
        };
        let MethodBody::User(function) = found.method.body else {
            return Step::Slow;
        };
        if found.method.is_abstract {
            return Step::Slow;
        }
        self.prepare_php_method(function, receiver, called, position.ip)
    }

    /// Record a PHP method call with `$this` in the window's first register.
    /// Static calls supply `called` for late static binding.
    fn prepare_php_method(
        &mut self,
        function: FunctionIndex,
        receiver: Option<Rc<Object>>,
        called: Option<ClassIndex>,
        ip: usize,
    ) -> Step {
        let this = receiver.map(Value::Object);
        let has_this = this.is_some();
        let parts = (&mut *self.stack, &mut *self.registers);
        push_php_call(self.code, parts, function, CallKind::Method, [this]);
        if !has_this
            && let Some(call) = self.stack.last_call_mut()
            && called != self.code.facts(function).class
        {
            call.called = called;
        }
        Step::Done(ip)
    }

    /// The class scope of the running frame (`self::`), if it has one.
    fn running_scope(&self) -> Option<&Rc<Class>> {
        self.current_context().and_then(|context| context.scope())
    }

    /// The class a named class symbol was linked to, once some earlier
    /// operation of this file resolved it.
    fn linked_class(&self, position: Position, symbol: SymbolId) -> Option<Rc<Class>> {
        let cache = self.code.cache(position.frame.function);
        cache.classes.get(symbol.index())?.borrow().clone()
    }

    /// The window of the running frame, if it is a method or closure.
    pub(super) fn current_context(&self) -> Option<WindowContext<'_>> {
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

    /// Whether the class scope has a different, private method of this
    /// name, which a call from inside that class reaches instead.
    fn shadowed_by_private(
        &self,
        scope: Option<&Rc<Class>>,
        key: NameKey,
        method: &Rc<crate::classes::Method>,
    ) -> bool {
        scope.is_some_and(|scope| {
            self.classes
                .method_table(scope)
                .private(key)
                .is_some_and(|found| !Rc::ptr_eq(found.method, method))
        })
    }
}

/// Whether a frame with this class scope may call the method without the
/// general visibility check: it is public; or private and called from
/// inside the class that declares it; or protected and called from the
/// declaring class, one of its parents or one of its descendants. PHP
/// allows a protected call in a few more cases (through the method's
/// prototype); those take the general path, which applies every rule.
fn may_call(found: &FoundMethod<'_>, scope: Option<&Rc<Class>>) -> bool {
    match found.method.visibility {
        Visibility::Public => true,
        Visibility::Private => {
            let scope = scope.map(|scope| scope.index);
            found
                .scope
                .upgrade()
                .is_some_and(|declaring| Some(declaring.index) == scope)
        }
        Visibility::Protected => scope.is_some_and(|scope| {
            found.scope.upgrade().is_some_and(|declaring| {
                descends_from(scope, &declaring) || descends_from(&declaring, scope)
            })
        }),
    }
}

/// Whether `class` is `ancestor` or extends it, following parent classes.
fn descends_from(class: &Rc<Class>, ancestor: &Class) -> bool {
    let mut current = Some(class);
    while let Some(candidate) = current {
        if candidate.index == ancestor.index {
            return true;
        }
        current = candidate.parent.as_ref();
    }
    false
}
