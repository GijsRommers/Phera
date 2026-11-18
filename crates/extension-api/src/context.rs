use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use phera_value::{Diagnostics, Object, PhpArray, Value};

use crate::{
    CompileDiagnostic, CompileDiagnosticKind, CompileDiagnosticSite, NativeError, NativeResult,
    RequestState,
};

/// A future that stays on the worker thread it was created on. PHP values are
/// reference counted with `Rc`, so nothing that holds one can be `Send`.
pub type LocalFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

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

    /// Validates a callable before a native operation starts its work.
    fn validate_callback(
        &mut self,
        _function: &str,
        _number: usize,
        _parameter: &str,
        _callable: &Value,
    ) -> Result<(), NativeError> {
        Err(NativeError::error(
            "Cannot resolve a PHP callback without a runtime",
        ))
    }

    /// Passes numeric arguments positionally and string keys as names.
    fn call_array(
        &mut self,
        _callable: &Value,
        _arguments: &PhpArray,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(std::future::ready(Err(NativeError::error(
            "Cannot call a PHP callback without a runtime",
        ))))
    }

    /// Invokes a public instance method selected by a language protocol.
    fn call_method(
        &mut self,
        object: Rc<Object>,
        name: &str,
        arguments: Vec<Value>,
    ) -> LocalFuture<'_, NativeResult> {
        drop(arguments);
        let message = format!(
            "Cannot call {}::{name}() without a runtime",
            object.class.name()
        );
        Box::pin(std::future::ready(Err(NativeError::error(message))))
    }

    /// Countable results are converted to integers even without a declared return type.
    fn count_object(&mut self, object: Rc<Object>) -> LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let value = self.call_method(object, "count", Vec::new()).await?;
            Ok(Value::Int(phera_value::to_int(&value, self.diagnostics())?))
        })
    }

    /// The class selected by a native static method call, including inherited factories.
    fn called_class(&self) -> Option<&str> {
        None
    }

    /// Projects a native object's properties for the requesting PHP operation.
    fn object_properties(
        &mut self,
        object: &Object,
        _purpose: crate::PropertyPurpose,
    ) -> Result<PhpArray, NativeError> {
        Ok(object.to_array())
    }

    /// Acquires a cursor for an object that implements Traversable.
    fn traverse(
        &mut self,
        object: Rc<Object>,
    ) -> LocalFuture<'_, crate::traversal::TraversalResult> {
        let message = format!("Cannot iterate {} without a runtime", object.class.name());
        Box::pin(std::future::ready(Err(NativeError::error(message))))
    }

    /// Wraps a native cursor in PHP's InternalIterator class.
    fn create_iterator(&mut self, _iterator: Box<dyn crate::NativeIterator>) -> NativeResult {
        Err(NativeError::error(
            "Cannot create an InternalIterator without a runtime",
        ))
    }

    /// Checks a factory target even when there will be no objects in the result.
    fn ensure_instantiable(&self, _class: &str) -> Result<(), NativeError> {
        Ok(())
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

    /// Access to the runtime's cold reflection queries.
    fn reflection(&mut self) -> Option<&mut dyn crate::reflection::ReflectionContext> {
        None
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
