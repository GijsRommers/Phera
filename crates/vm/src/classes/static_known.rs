//! Compiler-known static calls avoid materializing class and method PHP strings.

use super::{Invocation, static_sites::StaticNames};
use crate::{Vm, call_state::CallTarget};
use phera_bytecode::{ClassBinding, SymbolId};
use phera_extension_api::NativeError;
use std::rc::Rc;

impl Vm {
    /// Mirrors the dynamic static call protocol with compiler-resolved symbols.
    pub(crate) fn prepare_known_static_call(
        &mut self,
        class: SymbolId,
        method: SymbolId,
        binding: ClassBinding,
    ) -> Result<(), NativeError> {
        let context = self.static_context()?;
        if let Some(callee) = self.known_static_callee(&context) {
            self.bind_static_site(callee, context.receiver)?;
        } else {
            self.prepare_known_static_miss(class, method, binding)?;
            self.record_static_site(context, StaticNames::Known)?;
        }
        Ok(())
    }

    /// Visibility, abstract/static legality and receiver compatibility stay on cache misses.
    fn prepare_known_static_miss(
        &mut self,
        symbol: SymbolId,
        method: SymbolId,
        binding: ClassBinding,
    ) -> Result<(), NativeError> {
        let unit = self
            .current()
            .ok_or_else(|| internal_error!("known static frame"))?
            .unit
            .clone();
        let method = unit
            .symbols
            .get(method.index())
            .ok_or_else(|| internal_error!("known method symbol"))?;
        let name = self.class_symbol(symbol)?;
        let class = self
            .classes
            .get(&name)
            .ok_or_else(|| NativeError::error(format!("Class \"{name}\" not found")))?;
        let context = self
            .current()
            .and_then(|frame| frame.method.as_deref())
            .cloned();
        let receiver = context
            .as_ref()
            .and_then(|context| context.receiver.clone())
            .filter(|object| {
                self.classes
                    .for_object(object)
                    .is_some_and(|class| class.is_a(&name))
            });
        let resolved = self
            .cached_method_lookup(
                &class,
                &method.written,
                Some(&method.name),
                Invocation::Method,
            )?
            .ok_or_else(|| {
                NativeError::error(format!(
                    "Call to undefined method {}::{}()",
                    class.name, method.written
                ))
            })?;
        self.prepare_resolved_method(class, receiver, resolved, Invocation::Method)?;
        if binding == ClassBinding::Forward
            && let Some(context) = context
        {
            let pending = self
                .call_stack
                .current_mut()
                .and_then(|frame| frame.calls.last_mut())
                .ok_or_else(|| internal_error!("static forwarding"))?;
            if let CallTarget::Method {
                context: target, ..
            } = &mut Rc::make_mut(&mut pending.callee).target
            {
                target.called = context.called;
            }
        }
        Ok(())
    }
}
