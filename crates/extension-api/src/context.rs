use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use phera_value::{Diagnostics, Object, PhpArray, PhpString, Value};

use crate::{
    CompileDiagnostic, CompileDiagnosticKind, CompileDiagnosticSite, NativeError, NativeResult,
    RequestState,
};

/// A future that stays on the worker thread it was created on. PHP values are
/// reference counted with `Rc`, so nothing that holds one can be `Send`.
pub type LocalFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// A debug hook can inspect the same object again without re-entering itself.
pub enum DebugProperties {
    Properties(Rc<PhpArray>),
    Recursive,
}

/// What a native function may ask of the runtime that called it.
///
/// One context belongs to one running request. The VM implements it; tests use
/// [`crate::testing::TestContext`].
pub trait Context {
    /// Appends to the request's output, as `echo` does.
    fn write(&mut self, bytes: &[u8]);

    /// Where warnings, notices and deprecations go.
    fn diagnostics(&mut self) -> &mut dyn Diagnostics;

    /// Per-request storage for extensions; see [`RequestState`].
    fn state(&mut self) -> &mut RequestState;

    /// Whether the PHP file making this call declared `strict_types=1`, which
    /// turns off scalar coercion of arguments.
    fn strict_types(&self) -> bool;

    /// Calls a PHP callable (a closure, a function name, `[$object, 'method']`)
    /// and waits for its result.
    ///
    /// Only async native functions can await this, which is why anything taking
    /// a callback (`array_map`, `usort`) is an async native function. An error
    /// from the callback must be passed on unchanged.
    fn call<'a>(
        &'a mut self,
        callable: &'a Value,
        arguments: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult>;

    /// Convert a value to a PHP string. A VM context may run __toString.
    fn stringify<'a>(
        &'a mut self,
        value: &'a Value,
    ) -> LocalFuture<'a, Result<PhpString, NativeError>> {
        let result = phera_value::to_php_string(value, self.diagnostics()).map_err(Into::into);
        Box::pin(std::future::ready(result))
    }

    /// Loose comparisons may invoke __toString on mixed string/object pairs.
    fn compare<'a>(
        &'a mut self,
        left: &'a Value,
        right: &'a Value,
    ) -> LocalFuture<'a, Result<i8, NativeError>> {
        let result = phera_value::compare(left, right, self.diagnostics()).map_err(Into::into);
        Box::pin(std::future::ready(result))
    }

    /// Whether inspecting this object can run PHP code.
    fn has_debug_info(&self, _object: &Object) -> bool {
        false
    }

    /// Fetch inspection properties with the object's debug hook, if present.
    fn debug_properties<'a>(
        &'a mut self,
        object: Rc<Object>,
        _function: &'static str,
        _arguments: &[Value],
    ) -> LocalFuture<'a, Result<DebugProperties, NativeError>> {
        Box::pin(std::future::ready(Ok(DebugProperties::Properties(
            Rc::new(object.to_array()),
        ))))
    }

    /// Creates an object of the named class with these properties, without
    /// calling its constructor. This is how `json_decode` and `(object)`
    /// make a `stdClass`: classes belong to the VM, so native code asks for
    /// an object instead of building one.
    ///
    /// `properties` are keyed the way [`phera_value::Object`] stores them.
    /// The default refuses, for contexts that have no classes.
    fn create_object(&mut self, class: &str, properties: PhpArray) -> NativeResult {
        let _ = properties;
        Err(NativeError::error(format!(
            "An object of class {class} cannot be created here"
        )))
    }

    /// Publishes scanner diagnostics with PHP's severity and location.
    fn compile_diagnostic(
        &mut self,
        diagnostic: &CompileDiagnostic,
        site: CompileDiagnosticSite<'_>,
    ) {
        match diagnostic.kind {
            CompileDiagnosticKind::Warning => {
                let file = match site {
                    CompileDiagnosticSite::Caller => "Unknown",
                    CompileDiagnosticSite::Source(file) => file,
                };
                self.write(diagnostic.render(file, diagnostic.line).as_bytes());
            }
            CompileDiagnosticKind::Deprecated => self.deprecated(diagnostic.message.clone()),
        }
    }

    fn warning(&mut self, message: String) {
        self.diagnostics().warning(message);
    }

    fn notice(&mut self, message: String) {
        self.diagnostics().notice(message);
    }

    fn deprecated(&mut self, message: String) {
        self.diagnostics().deprecated(message);
    }
}
