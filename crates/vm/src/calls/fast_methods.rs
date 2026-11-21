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

use std::cell::Cell;
use std::rc::Rc;

use phera_bytecode::{
    ClassBinding, Instruction, MethodSite, MethodSiteId, NameId, Reg, SymbolId, Visibility,
};
use phera_value::{Object, Value};

use crate::calls::execution_loop::{Machine, Position, Step};
use crate::calls::frame::Frame;
use crate::calls::php_windows::push_php_call;
use crate::calls::prepared_call::CallKind;
use crate::calls::window_context::WindowContext;
use crate::classes::method_table::FoundMethod;
use crate::classes::{Class, ClassIndex, MethodBody};
use crate::code_table::{FunctionIndex, FunctionShape, NameKey};
use crate::unit_caches::{SiteOutcome, SiteRecord};

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
            // A getter with no parameters whose call comes next runs here.
            MethodBody::Native(entry)
                if entry.direct.is_some()
                    && entry.params.is_empty()
                    && let Some((destination, value)) =
                        self.run_direct_getter(position, object, (&found, key)) =>
            {
                self.registers
                    .put(position.frame.base() + destination.index(), value);
                Step::Done(position.ip + 1)
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

    /// `CallMethod`: what method site `site` found for the receiver's class
    /// the last time (its record) or else finds now. A PHP method is
    /// entered from here, with its arguments in a new window, and the caller
    /// goes on after the call; for anything else (a native method, `__call`,
    /// an error, a closure, a generator) the loop goes on with the
    /// `InitMethodCall`, sends and `Call` that follow.
    #[inline(never)]
    pub(super) fn call_method(
        &mut self,
        position: Position,
        site_id: MethodSiteId,
        destination: Reg,
    ) -> Step {
        let code = self.code;
        let (body, cache, facts) = code.function_parts(position.frame.function);
        let Some(site) = body.method_sites.get(site_id.index()) else {
            return Step::Slow;
        };
        let object = match self
            .registers
            .window(position.frame.base() + site.receiver.index(), 1)
        {
            [Some(Value::Object(object))] => object,
            _ => return Step::Done(position.ip),
        };
        let address = SiteRecord::address(&object.class);
        // A closure's scope can change from one call to the next, so its
        // sites keep no record.
        let record = match position.frame.kind() {
            CallKind::Closure => None,
            _ => cache.site_record(facts, body.method_sites.len(), site_id),
        };
        // The scope the site's code runs in (see `caller_scope`): a method's
        // declaring class, from its facts; none for a function.
        let scope = match position.frame.kind() {
            CallKind::Function => None,
            _ => facts.class,
        };
        let outcome = match record.map(Cell::get) {
            Some(SiteRecord {
                class,
                scope: recorded,
                outcome,
            }) if class == address && recorded == scope => outcome,
            _ => {
                let Some(class) = object.class.as_any().downcast_ref::<Class>() else {
                    return Step::Done(position.ip);
                };
                let outcome = self.resolve_site(position, site, class);
                if let Some(record) = record {
                    record.set(SiteRecord {
                        class: address,
                        scope,
                        outcome,
                    });
                }
                outcome
            }
        };
        match outcome {
            // The VM runs the following `InitMethodCall` itself.
            SiteOutcome::Vm => Step::Slow,
            SiteOutcome::Enter(function) => {
                let object = object.clone();
                self.enter_site_method(position, (body, site), function, object, destination)
            }
            // A native getter's value goes where the call's result goes; a
            // lazy object, or a getter that declines, takes the general
            // sequence.
            SiteOutcome::Getter(getter) => {
                if self.classes.has_lazy_objects && object.lazy().is_some() {
                    return Step::Done(position.ip);
                }
                let Some(value) = getter(object, &[]) else {
                    return Step::Done(position.ip);
                };
                self.registers
                    .put(position.frame.base() + destination.index(), value);
                Step::Done(site.after.index())
            }
            SiteOutcome::Native => {
                let object = object.clone();
                self.call_site_native(position, (body, site), &object, destination)
            }
        }
    }

    /// What method site `site` does for a receiver of class `class`: the
    /// method found by name, and whether the site's scope may call it. It
    /// depends on nothing else, so the site records it.
    #[cold]
    #[inline(never)]
    fn resolve_site(&self, position: Position, site: &MethodSite, class: &Class) -> SiteOutcome {
        let key = self
            .code
            .unit_name_key(position.frame.function, site.method);
        let Some(found) = self.classes.method_table(class).get(key) else {
            return SiteOutcome::Vm;
        };
        let method = found.method;
        if method.is_abstract || method.is_static {
            return SiteOutcome::Vm;
        }
        match method.body {
            MethodBody::User(function) => {
                let scope = self.caller_scope(position);
                if may_call(&found, scope) && !self.shadowed_by_private(scope, key, method) {
                    SiteOutcome::Enter(function)
                } else {
                    SiteOutcome::Vm
                }
            }
            // `Closure::__invoke` and the methods of a generator have their
            // own preparation in the VM.
            MethodBody::Native(_) if class.name == "Closure" || class.name == "Generator" => {
                SiteOutcome::Vm
            }
            // A getter, as `run_direct_getter` takes it for `InitMethodCall`.
            MethodBody::Native(entry)
                if site.arguments == 0
                    && entry.params.is_empty()
                    && method.visibility == Visibility::Public
                    && !self.shadowed_by_private(self.caller_scope(position), key, method)
                    && let Some(getter) = entry.direct =>
            {
                SiteOutcome::Getter(getter)
            }
            MethodBody::Native(_) => SiteOutcome::Native,
            _ => SiteOutcome::Vm,
        }
    }

    /// A method site whose method is native: found again by its key (the
    /// record says only that it is native), prepared as the following
    /// `InitMethodCall` prepares it, given its arguments as the sends give
    /// them, and called as `Call` calls it; the written-out sequence from
    /// whichever of these needs more than the loop does.
    #[cold]
    #[inline(never)]
    fn call_site_native(
        &mut self,
        position: Position,
        (body, site): (&phera_bytecode::Function, &MethodSite),
        object: &Rc<Object>,
        destination: Reg,
    ) -> Step {
        let Some(class) = object.class.as_any().downcast_ref::<Class>() else {
            return Step::Done(position.ip);
        };
        let key = self
            .code
            .unit_name_key(position.frame.function, site.method);
        let Some(found) = self.classes.method_table(class).get(key) else {
            return Step::Done(position.ip);
        };
        // Owned, so that the preparation may change the machine.
        let found = (found.method.clone(), found.scope.clone());
        let sends = Position {
            frame: position.frame,
            ip: position.ip + 1,
        };
        match self.prepare_native_site(sends, object, (class, key, found)) {
            Step::Done(next) if next == sends.ip => {}
            Step::Slow => return Step::Done(position.ip),
            step => return step,
        }
        // The sends, as the loop makes them; if any is refused, none is
        // made and the written-out sends make them all.
        let Some(arguments) = body.site_arguments.get(site.argument_range()) else {
            return Step::Done(sends.ip);
        };
        // Every argument is tested before any is sent: `send` writes at the
        // next position, so the written-out sends, which start from the
        // first argument, must find none sent yet.
        let Some(call) = self.stack.last_call().copied() else {
            return Step::Done(sends.ip);
        };
        let base = position.frame.base();
        for (index, argument) in arguments.iter().enumerate() {
            let accepted = match self.registers.window(base + argument.register().index(), 1) {
                [Some(value)] => self.other_accepts(call.callee, index, &value.deref()),
                _ => false,
            };
            if !accepted {
                return Step::Done(sends.ip);
            }
        }
        for argument in arguments {
            if !matches!(self.send(position, argument.register()), Step::Done(_)) {
                return Step::Done(sends.ip);
            }
        }
        // The call, as the loop makes it (a direct native), going on after
        // the written-out `Call`; one that needs the VM goes there through
        // that `Call`.
        let after = Position {
            frame: position.frame,
            ip: site.after.index(),
        };
        let direct = self.stack.last_call().is_some_and(|call| {
            call.flags
                .contains(crate::calls::prepared_call::CallFlags::DIRECT_NATIVE)
        });
        if !direct {
            return Step::Done(after.ip - 1);
        }
        match self.enter_prepared(after, destination) {
            Step::Slow => Step::Done(after.ip - 1),
            step => step,
        }
    }

    /// Enter the PHP method `function` of method site `site` with `object`
    /// as `$this`: plain in shape, with arguments that are defined, not
    /// references, passed by value and accepted by their types. Otherwise
    /// the call becomes the prepared call the following `InitMethodCall`
    /// makes, and the sends after it run next.
    #[inline(always)]
    fn enter_site_method(
        &mut self,
        position: Position,
        (body, site): (&phera_bytecode::Function, &MethodSite),
        function: FunctionIndex,
        object: Rc<Object>,
        destination: Reg,
    ) -> Step {
        // The sends, after the `InitMethodCall` that follows `CallMethod`.
        let sends = position.ip + 1;
        let Some(arguments) = body.site_arguments.get(site.argument_range()) else {
            return Step::Slow;
        };
        let count = arguments.len();
        let facts = self.code.facts(function);
        let base = position.frame.base();
        // The window, as `InitMethodCall` reserves it, with `$this`.
        let start = self.registers.len();
        let first = start + CallKind::Method.prefix();
        let window = (facts.registers.max(facts.parameters) as usize).max(1);
        self.registers.grow_to(first + window);
        self.registers.put(start, Value::Object(object));
        if facts.shape != FunctionShape::Plain
            || count < facts.required as usize
            || count > facts.parameters as usize
            || self.stack.depth() >= self.frame_limit
        {
            return self.record_site_call(function, start, sends);
        }
        for (index, argument) in arguments.iter().enumerate() {
            // A by-reference parameter binds to the variable itself, which
            // only the written-out `SendLocal` does.
            if !facts.takes_value(index) {
                return self.record_site_call(function, start, sends);
            }
            let value = match self.registers.window(base + argument.register().index(), 1) {
                [Some(value)] if !value.is_reference() => value,
                _ => return self.record_site_call(function, start, sends),
            };
            let plain = !facts.is_typed(index)
                || facts.parameter_masks[index].intersects(crate::types::own_type(value));
            let mut value = value.clone();
            if !plain
                && !self.accepts_argument(function, index, &value)
                && !self.accepts_on_second_look(function, index, &mut value)
            {
                return self.record_site_call(function, start, sends);
            }
            self.registers.put(first + index, value);
        }
        // The caller goes on after the call's `Call`.
        self.publish(site.after.index());
        let mut frame = Frame::of_kind(function, CallKind::Method, first, Some(destination));
        if count < facts.parameters as usize {
            frame = frame.with_passed(count);
        }
        self.stack.push(frame);
        if self.spend_work() {
            return Step::Yield;
        }
        Step::Switched(Position { frame, ip: 0 })
    }

    /// A method site whose PHP method cannot be entered here: its window
    /// (with `$this`, and any arguments already copied, which the sends
    /// write again) becomes the prepared call `InitMethodCall` makes, and
    /// the sends at `sends` run next.
    #[cold]
    #[inline(never)]
    fn record_site_call(&mut self, function: FunctionIndex, start: usize, sends: usize) -> Step {
        let call = crate::calls::prepared_call::PreparedCall::of_kind(
            crate::calls::prepared_call::PreparedCallee::Php(function),
            CallKind::Method,
            start,
        );
        self.stack.push_call(call, None);
        Step::Done(sends)
    }

    /// A method site whose method is native: prepared as `InitMethodCall`
    /// prepares it, with `sends` as the position after it (a getter with no
    /// arguments runs at once there, its `Call` being next). `Step::Slow`
    /// for whatever that preparation leaves to the VM.
    #[cold]
    #[inline(never)]
    fn prepare_native_site(
        &mut self,
        sends: Position,
        object: &Rc<Object>,
        (class, key, (method, scope)): (&Class, NameKey, FoundParts),
    ) -> Step {
        let found = FoundMethod {
            method: &method,
            scope: &scope,
        };
        self.prepare_found_native(sends, object, (class, key), &found)
    }

    /// `InitMethodCall` of a native method, as [`Machine::prepare_method`]
    /// prepares it, for a method site that found the method by `key`: a
    /// getter with no parameters whose `Call` comes next runs at once, any
    /// other public native method is described for its `Call`.
    fn prepare_found_native(
        &mut self,
        position: Position,
        object: &Rc<Object>,
        (class, key): (&Class, NameKey),
        found: &FoundMethod<'_>,
    ) -> Step {
        let method = found.method;
        let MethodBody::Native(entry) = method.body else {
            return Step::Slow;
        };
        if method.is_abstract
            || method.is_static
            || class.name == "Closure"
            || class.name == "Generator"
        {
            return Step::Slow;
        }
        if entry.direct.is_some()
            && entry.params.is_empty()
            && let Some((destination, value)) =
                self.run_direct_getter(position, object, (found, key))
        {
            self.registers
                .put(position.frame.base() + destination.index(), value);
            return Step::Done(position.ip + 1);
        }
        let Some((context, method)) = self.native_class_method((object, class), found, key) else {
            return Step::Slow;
        };
        match entry.direct {
            Some(_) => self.prepare_direct_method(position, context, method, entry),
            None => self.prepare_native_method(position, context, method),
        }
    }

    /// The class scope of the code at `position`: for a method, the class
    /// that declares it, read from the function's facts; a closure's takes
    /// the general way.
    #[inline(always)]
    fn caller_scope(&self, position: Position) -> Option<&Rc<Class>> {
        match position.frame.kind() {
            CallKind::Function => None,
            CallKind::Method | CallKind::Constructor => self
                .code
                .facts(position.frame.function)
                .class
                .map(|index| self.classes.class(index)),
            CallKind::Closure => self.running_scope(),
        }
    }

    /// `$object->getter()` of a public native method with no parameters
    /// and a direct handler (`$method->getName()`), when the instruction
    /// after this `InitMethodCall` is its `Call`: the handler runs now and
    /// its value goes where the `Call` puts its result, with no prepared
    /// call at all, and both instructions are done: the register and the
    /// value. `None` when the call is not that shape or the handler
    /// declines, for the ordinary path.
    #[inline(never)]
    fn run_direct_getter(
        &self,
        position: Position,
        object: &Rc<Object>,
        (found, key): (&FoundMethod<'_>, NameKey),
    ) -> Option<(Reg, Value)> {
        let method = found.method;
        let MethodBody::Native(entry) = method.body else {
            return None;
        };
        let direct = entry.direct?;
        let Some(Instruction::Call { dst, .. }) = self
            .code
            .instructions(position.frame.function)
            .get(position.ip)
        else {
            return None;
        };
        let destination = *dst;
        if method.visibility != Visibility::Public
            || (self.classes.has_lazy_objects && object.lazy().is_some())
            || self.shadowed_by_private(self.running_scope(), key, method)
        {
            return None;
        }
        Some((destination, direct(object, &[])?))
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

    /// The class a named class symbol of the running file means in this
    /// request, by the symbol's process name key.
    pub(super) fn linked_class(&self, position: Position, symbol: SymbolId) -> Option<Rc<Class>> {
        let key = self.code.unit_symbol_key(position.frame.function, symbol);
        self.classes.by_key(key).cloned()
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

/// A found method held by value: the method and the class that declares it.
type FoundParts = (Rc<crate::classes::Method>, std::rc::Weak<Class>);
