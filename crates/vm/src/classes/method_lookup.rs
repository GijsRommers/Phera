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
        let private = caller
            .as_ref()
            .filter(|scope| invocation == Invocation::Method && class.is_a(&scope.name))
            .and_then(|scope| scope.methods.get(lower))
            .filter(|method| method.visibility == Visibility::Private);
        let Some(method) = private.or_else(|| class.methods.get(lower)).cloned() else {
            return Ok(None);
        };
        self.check_method_visibility(&method)?;
        if class.kind == ClassKind::Trait && method.is_static {
            return Err(super::trait_methods::deprecated_direct_access(
                "method",
                &format!("{}::{}()", class.name, method.name),
            ));
        }
        let scope = declaring_class(class, &method)
            .or_else(|| self.classes.get(&method.declaring))
            .ok_or_else(|| internal_error!("method scope"))?;
        Ok(Some(ResolvedMethod { method, scope }))
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
