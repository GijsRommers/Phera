//! The general method lookup: what the interpreter loop's fast paths do not
//! take (protected methods, `__invoke` of closures, names not interned).
//!
//! ```php
//! class A { private function f() {} public function g() { $this->f(); } }
//! class B extends A { public function f() {} }
//! (new B)->g();   // calls A::f: a private method of the calling class wins
//! ```

use super::{Class, Invocation, Method};
use crate::Vm;
use phera_bytecode::{ClassKind, Visibility};
use phera_extension_api::NativeError;
use std::rc::Rc;

/// A method and the class that declares it, which becomes the call's scope.
pub(crate) struct ResolvedMethod {
    pub(crate) method: Rc<Method>,
    pub(crate) scope: Rc<Class>,
}

impl Vm {
    /// Find `name` in `class` as a call from the running frame sees it, and
    /// check that the running frame may call it.
    pub(crate) fn find_method(
        &mut self,
        class: &Rc<Class>,
        name: &str,
        invocation: Invocation,
    ) -> Result<Option<ResolvedMethod>, NativeError> {
        let lower = match name.bytes().any(|byte| byte.is_ascii_uppercase()) {
            true => std::borrow::Cow::Owned(name.to_ascii_lowercase()),
            false => std::borrow::Cow::Borrowed(name),
        };
        self.find_method_lowercase(class, &lower, invocation)
    }

    /// [`Vm::find_method`] for a name that is lowercase already.
    pub(crate) fn find_method_lowercase(
        &mut self,
        class: &Rc<Class>,
        lower: &str,
        invocation: Invocation,
    ) -> Result<Option<ResolvedMethod>, NativeError> {
        let caller = self.current().and_then(|frame| frame.scope().cloned());
        self.find_method_with_visibility(class, lower, invocation, caller.as_ref(), |vm, method| {
            vm.check_invocation_visibility(method, invocation)
        })
    }

    pub(super) fn find_method_in_scope(
        &mut self,
        class: &Rc<Class>,
        name: &str,
        scope: Option<&Rc<Class>>,
    ) -> Result<Option<ResolvedMethod>, NativeError> {
        self.find_method_with_visibility(
            class,
            &name.to_ascii_lowercase(),
            Invocation::Method,
            scope,
            |vm, method| vm.check_visibility_in_scope(method, Invocation::Method, scope),
        )
    }

    fn find_method_with_visibility(
        &mut self,
        class: &Rc<Class>,
        lower: &str,
        invocation: Invocation,
        caller: Option<&Rc<Class>>,
        check: impl FnOnce(&Self, &Method) -> Result<(), NativeError>,
    ) -> Result<Option<ResolvedMethod>, NativeError> {
        let private = caller
            .filter(|scope| invocation == Invocation::Method && class.is_a(&scope.name))
            .and_then(|scope| scope.methods.get(lower))
            .filter(|method| method.visibility == Visibility::Private);
        let Some(method) = private.or_else(|| class.methods.get(lower)).cloned() else {
            return Ok(None);
        };
        check(self, &method)?;
        if class.kind == ClassKind::Trait && method.is_static {
            self.deprecate_static_trait_method(class, &method.name);
        }
        let scope = declaring_class(class, &method)
            .or_else(|| self.classes.get(&method.declaring))
            .ok_or_else(|| internal_error!("method scope"))?;
        Ok(Some(ResolvedMethod { method, scope }))
    }
    /// Direct static access through a trait also warns when PHP uses __callStatic.
    pub(super) fn deprecate_static_trait_method(&mut self, class: &Class, name: &str) {
        self.diagnostic(
            "Deprecated",
            format!(
                "Calling static trait method {}::{name} is deprecated, it should only be called on a class using the trait",
                class.name,
            ),
        );
    }
}

/// The class among `class` and its ancestors that declares `method`.
pub(crate) fn declaring_class(class: &Rc<Class>, method: &Method) -> Option<Rc<Class>> {
    let mut candidate = Some(class);
    while let Some(class) = candidate {
        if class.name == method.declaring {
            return Some(class.clone());
        }
        candidate = class.parent.as_ref();
    }
    None
}
