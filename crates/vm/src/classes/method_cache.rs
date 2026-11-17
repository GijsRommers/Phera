//! Method sites cache immutable resolution without retaining receiver objects.

use super::{Class, Invocation, Method};
use crate::Vm;
use phera_bytecode::Visibility;
use phera_extension_api::NativeError;
use std::rc::Rc;

/// The receiver lookup class, lexical caller scope, spelling and invocation guard
/// each entry. Entries are made only after successful lookup and visibility checks.
/// Linked classes/methods are immutable; mismatches replace the entry. Retained
/// classes and UnitCaches prevent address reuse; maps die at request end.
pub(crate) struct MethodEntry {
    pub(crate) class: Rc<Class>,
    caller: Option<Rc<Class>>,
    name: String,
    invocation: Invocation,
    method: Rc<Method>,
    scope: Rc<Class>,
}

/// Scope and method metadata can be shared while every call binds its own receiver.
pub(crate) struct ResolvedMethod {
    pub(crate) method: Rc<Method>,
    pub(crate) scope: Rc<Class>,
}

impl Vm {
    /// Resolve and check visibility once for each guarded class/scope combination.
    pub(crate) fn cached_method(
        &mut self,
        class: &Rc<Class>,
        name: &str,
        invocation: Invocation,
    ) -> Result<Option<ResolvedMethod>, NativeError> {
        self.cached_method_lookup(class, name, None, invocation)
    }

    /// Known symbols provide their compiler-normalized lookup spelling separately
    /// from diagnostic spelling; dynamic names normalize only on a cache miss.
    pub(crate) fn cached_method_lookup(
        &mut self,
        class: &Rc<Class>,
        name: &str,
        lookup: Option<&str>,
        invocation: Invocation,
    ) -> Result<Option<ResolvedMethod>, NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("method site"))?;
        let caller = frame.method().map(|context| context.scope.clone());
        if let Some(slot) = frame.site_cache()
            && let crate::unit_caches::CachedSite::Method(entry) = &*slot.borrow()
            && Rc::ptr_eq(&entry.class, class)
            && entry.name == name
            && entry.invocation == invocation
            && same_scope(&entry.caller, &caller)
        {
            return Ok(Some(ResolvedMethod {
                method: entry.method.clone(),
                scope: entry.scope.clone(),
            }));
        }
        let lower = lookup
            .map(std::borrow::Cow::Borrowed)
            .unwrap_or_else(|| std::borrow::Cow::Owned(name.to_ascii_lowercase()));
        let private = caller
            .as_ref()
            .filter(|scope| invocation == Invocation::Method && class.is_a(&scope.name))
            .and_then(|scope| scope.methods.get(lower.as_ref()))
            .filter(|method| method.visibility == Visibility::Private);
        let Some(method) = private
            .or_else(|| class.methods.get(lower.as_ref()))
            .cloned()
        else {
            return Ok(None);
        };
        self.check_method_visibility(&method)?;
        let scope = self
            .classes
            .get(&method.declaring)
            .ok_or_else(|| internal_error!("method scope"))?;
        if let Some(slot) = self.current().and_then(|frame| frame.site_cache()) {
            *slot.borrow_mut() = crate::unit_caches::CachedSite::Method(Box::new(MethodEntry {
                class: class.clone(),
                caller,
                name: name.to_owned(),
                invocation,
                method: method.clone(),
                scope: scope.clone(),
            }));
        }
        Ok(Some(ResolvedMethod { method, scope }))
    }
}

/// Global callers and lexical class callers occupy distinct visibility domains.
pub(crate) fn same_scope(left: &Option<Rc<Class>>, right: &Option<Rc<Class>>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => Rc::ptr_eq(left, right),
        _ => false,
    }
}
