//! Method lookup uses the same integer slot table for literal and computed names.
//!
//! In `Parent::run()` and `$child->run()`, visibility uses the lexical caller,
//! while the receiver chooses the virtual slot. No call-site state is retained.

use super::Invocation;
use crate::{Vm, names::Name};
use phera_bytecode::Visibility;
use phera_extension_api::NativeError;

/// A lookup result carries declaration scope separately from receiver class.
pub(crate) struct ResolvedMethod {
    pub(crate) method: super::method_table::MethodAddress,
    pub(crate) scope: super::class_table::ClassIndex,
}

impl Vm {
    /// Computed names intern on their way into the shared slot resolver.
    pub(crate) fn resolve_named_method(
        &mut self,
        class: super::class_table::ClassIndex,
        name: &str,
        invocation: Invocation,
    ) -> Result<Option<ResolvedMethod>, NativeError> {
        let name = self.classes.names.intern(name);
        self.resolve_method(class, name, invocation)
    }

    /// Private lexical methods take precedence only in a compatible receiver hierarchy.
    pub(crate) fn resolve_method(
        &self,
        class: super::class_table::ClassIndex,
        name: Name,
        invocation: Invocation,
    ) -> Result<Option<ResolvedMethod>, NativeError> {
        let caller = self
            .current()
            .and_then(|frame| self.call_stack.extras[frame.depth].method.as_ref())
            .map(|context| context.scope);
        let class_metadata = self.classes.at(class);
        let private = caller
            .filter(|scope| invocation == Invocation::Method && class_metadata.includes(*scope))
            .and_then(|scope| {
                self.classes
                    .at(scope)
                    .methods
                    .find(name)
                    .map(|slot| (scope, slot))
            })
            .filter(|(scope, slot)| {
                self.classes.at(*scope).methods.at(*slot).visibility == Visibility::Private
            });
        let resolved =
            private.or_else(|| class_metadata.methods.find(name).map(|slot| (class, slot)));
        let Some((class, slot)) = resolved else {
            return Ok(None);
        };
        let method = self.classes.at(class).methods.at(slot);
        self.check_method_visibility(method)?;
        let function = match &method.body {
            super::MethodBody::User(reference) => Some(reference.address),
            _ => None,
        };
        Ok(Some(ResolvedMethod {
            method: super::method_table::MethodAddress {
                class,
                slot,
                function,
                is_static: method.is_static,
            },
            scope: method.declaring_index,
        }))
    }
}
