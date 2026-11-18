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

/// Properties provided to an inspection operation that may run PHP code.
pub enum PropertyView {
    Snapshot(Rc<PhpArray>),
    DebugInfo(Rc<PhpArray>),
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

    /// The PHP file making this call, for include-path lookups.
    fn script_path(&self) -> Option<std::path::PathBuf> {
        None
    }

    /// The request's include path, used by the VM and by the file wrappers.
    fn include_path(&self) -> phera_value::PhpString {
        ".".into()
    }

    /// Whether the PHP file making this call declared `strict_types=1`, which
    /// turns off scalar coercion of arguments.
    fn strict_types(&self) -> bool;

    /// Attach the caller-visible function frame to an error raised by an
    /// async native handler itself. A PHP callback already owns its trace.
    fn native_call_error(
        &mut self,
        _name: &str,
        _arguments: &[Value],
        error: NativeError,
    ) -> NativeError {
        error
    }

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

    /// Calls an instance method with ordinary PHP method dispatch.
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

    /// Invokes an overridable method in a native class's own inheritance scope.
    fn call_protected_method(
        &mut self,
        object: Rc<Object>,
        name: &str,
        arguments: Vec<Value>,
    ) -> LocalFuture<'_, NativeResult> {
        self.call_method(object, name, arguments)
    }

    /// Invokes a public instance method required by a PHP language protocol.
    fn call_protocol_method(
        &mut self,
        object: Rc<Object>,
        name: &str,
        arguments: Vec<Value>,
    ) -> LocalFuture<'_, NativeResult> {
        self.call_method(object, name, arguments)
    }

    /// Throws an exception object the script itself made, as `throw $object`
    /// would. The returned error is what the native function hands back.
    fn throw_object(&mut self, _object: Rc<Object>) -> NativeError {
        NativeError::error("Cannot throw an object without a runtime")
    }

    /// Countable results are converted to integers even without a declared return type.
    fn count_object(&mut self, object: Rc<Object>) -> LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let value = self
                .call_protocol_method(object, "count", Vec::new())
                .await?;
            Ok(Value::Int(phera_value::to_int(&value, self.diagnostics())?))
        })
    }

    /// Resolves a class name, including autoloading and class aliases.
    fn lookup_class<'a>(
        &'a mut self,
        _name: &'a str,
    ) -> LocalFuture<'a, Result<Option<Rc<dyn phera_value::ClassHandle>>, NativeError>> {
        Box::pin(std::future::ready(Ok(None)))
    }

    /// Tests whether a language operation should dispatch to a PHP implementation.
    fn is_user_method(&self, _object: &Object, _name: &str) -> bool {
        false
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

    /// Whether the requesting operation will need to run a PHP property hook.
    fn property_view_runs_code(&self, _object: &Object, _purpose: crate::PropertyPurpose) -> bool {
        false
    }

    /// Whether a stored property exists and is visible to the current PHP caller.
    fn has_visible_property(&self, object: &Object, name: &str) -> bool {
        object.has(&phera_value::ArrayKey::str(name))
    }

    /// Retrieves a property view for an inspector that can await a PHP hook.
    fn property_view(
        &mut self,
        object: Rc<Object>,
        purpose: crate::PropertyPurpose,
        _function: &'static str,
        _arguments: &[Value],
    ) -> LocalFuture<'_, Result<PropertyView, NativeError>> {
        let view = self
            .object_properties(&object, purpose)
            .map(|properties| PropertyView::Snapshot(Rc::new(properties)));
        Box::pin(std::future::ready(view))
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

    /// Warnings containing PHP strings may need to preserve non-UTF-8 bytes.
    fn warning_bytes(&mut self, message: &[u8]) {
        self.warning(String::from_utf8_lossy(message).into_owned());
    }

    fn notice(&mut self, message: String) {
        self.diagnostics().notice(message);
    }

    fn deprecated(&mut self, message: String) {
        self.diagnostics().deprecated(message);
    }
}
