//! Compiler-known static calls avoid materializing class and method PHP strings.

use super::Invocation;
use crate::Vm;
use phera_bytecode::{ClassBinding, SymbolId};
use phera_extension_api::NativeError;

impl Vm {
    /// Compiler-resolved symbols avoid repeating name conversion during static preparation.
    pub(crate) fn prepare_known_static_call(
        &mut self,
        class: SymbolId,
        method: SymbolId,
        binding: ClassBinding,
    ) -> Result<(), NativeError> {
        self.prepare_known_static(class, method, binding)
    }

    /// Visibility, abstract/static legality and receiver compatibility.
    fn prepare_known_static(
        &mut self,
        symbol: SymbolId,
        method: SymbolId,
        binding: ClassBinding,
    ) -> Result<(), NativeError> {
        let unit = self
            .current()
            .ok_or_else(|| internal_error!("known static frame"))?
            .unit()
            .clone();
        let method = unit
            .symbols
            .get(method.index())
            .ok_or_else(|| internal_error!("known method symbol"))?;
        let name = self.class_symbol(symbol)?;
        let class = self.classes.get(&name).ok_or_else(|| {
            Self::class_member_miss(
                &name,
                phera_source::deprecations::ItemKind::Method,
                &method.written,
            )
        })?;
        self.link_class_symbol(symbol, &class);
        let context = self.current().and_then(|frame| frame.method());
        let receiver = context
            .as_ref()
            .and_then(|context| context.receiver.clone())
            .filter(|object| {
                self.classes
                    .for_object(object)
                    .is_some_and(|class| self.class_matches(&class, &name))
            });
        let resolved = self
            .find_method_lowercase(&class, &method.name, Invocation::Method)?
            .ok_or_else(|| crate::unsupported_items::undefined_method(&class, &method.written))?;
        self.prepare_resolved_method(class, receiver, resolved, Invocation::Method)?;
        if binding == ClassBinding::Forward
            && let Some(context) = context
        {
            self.forward_callback_class(context.called)?;
        }
        Ok(())
    }
}
