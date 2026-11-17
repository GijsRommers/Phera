//! Static-style method sites share unbound targets after visibility resolution.

use super::Class;
use crate::{
    Vm,
    calls::call_state::{CallTarget, Callee},
};
use phera_bytecode::{ClassBinding, Reg};
use phera_extension_api::NativeError;
use phera_value::{ClassHandle, Object, Value};
use std::rc::Rc;

/// Known names are immutable instruction operands; dynamic names need byte guards.
pub(crate) enum StaticNames {
    Known,
    Runtime { class: String, method: String },
}

/// A caller snapshot carries the current receiver only while binding a call.
pub(crate) struct StaticContext {
    scope: Option<Rc<Class>>,
    called: Option<Rc<Class>>,
    pub(crate) receiver: Option<Rc<Object>>,
}

/// Made after successful preparation and visibility checking. Entries are keyed
/// by unit/function/IP, guarded by lexical scope, called class, receiver class,
/// and dynamic names. Targets never retain a receiver. Mismatches replace an
/// entry; immutable retained classes/units cannot change identity, and request
/// teardown discards the map. Forwarded contexts are therefore never reused in
/// a different scope or with an incompatible receiver.
pub(crate) struct StaticEntry {
    names: StaticNames,
    scope: Option<Rc<Class>>,
    called: Option<Rc<Class>>,
    callee: Callee,
    receiver_class: Option<Rc<dyn ClassHandle>>,
}

impl Vm {
    /// Dynamic static targets retain conversion diagnostics when a guarded site misses.
    pub(crate) fn prepare_static_site(
        &mut self,
        class: Reg,
        method: Reg,
        binding: ClassBinding,
    ) -> Result<(), NativeError> {
        let context = self.static_context()?;
        let cached = self
            .current()
            .and_then(|frame| frame.site_cache())
            .and_then(|slot| {
                let entry = slot.borrow();
                let crate::unit_caches::CachedSite::Static(entry) = &*entry else {
                    return None;
                };
                if !matches_context(entry, &context) {
                    return None;
                }
                match (&entry.names, self.get(class), self.get(method)) {
                    (
                        StaticNames::Runtime {
                            class: name,
                            method: member,
                        },
                        Value::String(class),
                        Value::String(method),
                    ) if class.as_bytes() == name.as_bytes()
                        && method.as_bytes() == member.as_bytes() =>
                    {
                        Some(entry.callee.clone())
                    }
                    _ => None,
                }
            });
        if let Some(callee) = cached {
            return self.bind_static_site(callee, context.receiver);
        }
        let class = self.class_target(class)?;
        let method = self.method_name(method)?;
        self.prepare_static_style(&class, &method, binding)?;
        self.record_static_site(context, StaticNames::Runtime { class, method })
    }

    /// Borrow-free snapshots let cache guards finish before mutating call buffers.
    pub(crate) fn static_context(&self) -> Result<StaticContext, NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("static site"))?;
        Ok(StaticContext {
            scope: frame.method().map(|context| context.scope.clone()),
            called: frame.method().map(|context| context.called.clone()),
            receiver: frame
                .method()
                .as_ref()
                .and_then(|context| context.receiver.clone()),
        })
    }

    /// Known symbols need only identity guards; their spelling is fixed by the instruction.
    pub(crate) fn known_static_callee(&self, context: &StaticContext) -> Option<Callee> {
        self.current()
            .and_then(|frame| frame.site_cache())
            .and_then(|slot| {
                let entry = slot.borrow();
                let crate::unit_caches::CachedSite::Static(entry) = &*entry else {
                    return None;
                };
                matches_context(entry, context).then(|| entry.callee.clone())
            })
    }

    /// Strip the receiver from a prepared target before retaining it in the cache.
    pub(crate) fn record_static_site(
        &mut self,
        caller: StaticContext,
        names: StaticNames,
    ) -> Result<(), NativeError> {
        let callee = self
            .call_stack
            .preparing
            .last()
            .map(|call| &call.callee)
            .ok_or_else(|| internal_error!("static prepared call"))?;
        if let CallTarget::Method { method, context } = &callee.target {
            let receiver_class = context.receiver.as_ref().map(|object| object.class.clone());
            let mut context = context.clone();
            context.receiver = None;
            let unbound = Callee {
                argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
                name: callee.name.clone(),
                parameters: callee.parameters.clone(),
                target: CallTarget::Method {
                    method: method.clone(),
                    context,
                },
            };
            if let Some(slot) = self.current().and_then(|frame| frame.site_cache()) {
                *slot.borrow_mut() =
                    crate::unit_caches::CachedSite::Static(Box::new(StaticEntry {
                        names,
                        scope: caller.scope,
                        called: caller.called,
                        callee: unbound,
                        receiver_class,
                    }));
            }
        }
        Ok(())
    }

    /// Cached visibility and scope stay valid; only this call's receiver changes.
    pub(crate) fn bind_static_site(
        &mut self,
        mut callee: Callee,
        receiver: Option<Rc<Object>>,
    ) -> Result<(), NativeError> {
        if let CallTarget::Method { method, context } = &mut callee.target
            && !method.is_static
        {
            context.receiver = receiver;
        }
        self.prepare_call(callee)
    }
}

/// No object identity is cached: only the class identity needed for compatibility.
fn matches_context(entry: &StaticEntry, caller: &StaticContext) -> bool {
    super::method_cache::same_scope(&entry.scope, &caller.scope)
        && super::method_cache::same_scope(&entry.called, &caller.called)
        && entry.receiver_class.as_ref().is_none_or(|class| {
            caller
                .receiver
                .as_ref()
                .is_some_and(|object| Rc::ptr_eq(class, &object.class))
        })
}
