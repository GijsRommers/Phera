use std::future::Future;
use std::pin::Pin;
use std::{cell::RefCell, rc::Rc};

use phera_value::{ArrayKey, Diagnostics, Object, PhpArray, PhpString, Value};

use crate::{
    CompileDiagnostic, CompileDiagnosticKind, CompileDiagnosticSite, NativeError, NativeResult,
    RequestState,
};

/// A future that stays on the worker thread it was created on. PHP values are
/// reference counted with `Rc`, so nothing that holds one can be `Send`.
pub type LocalFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Generator operations that can resume PHP code, through a method or iterator cursor.
pub enum GeneratorOperation {
    Method(crate::traversal::IteratorStep),
    Cursor {
        step: crate::traversal::IteratorStep,
        by_reference: bool,
    },
    Send(Value),
    Throw(Value),
    GetReturn,
}

/// When a callback registered by PHP runs.
#[derive(Clone, Copy)]
pub enum RequestCallback {
    Headers,
    Shutdown,
}

/// Properties provided to an inspection operation that may run PHP code.
pub enum PropertyView {
    Snapshot(Rc<PhpArray>),
    DebugInfo(Rc<PhpArray>),
    /// var_dump displays typed slots even when the object does not store a value.
    DebugSlots {
        slots: Rc<PhpArray>,
        uninitialized: std::collections::HashMap<phera_value::ArrayKey, String>,
    },
    Recursive,
}

/// What a native function may ask of the runtime that called it.
///
/// One context belongs to one running request. The VM implements it; tests use
/// [`crate::testing::TestContext`].
pub trait Context {
    /// Appends to the request's output, as `echo` does.
    fn write(&mut self, bytes: &[u8]);

    /// Snapshot PHP callers without retaining references to live parameter slots.
    fn backtrace(&mut self, _objects: bool, _arguments: bool, _limit: i32) -> Value {
        Value::from(PhpArray::new())
    }

    /// Print a trace using the request's exception string parameter limit.
    fn print_backtrace(&mut self, _trace: &Value) {}

    /// The most recent diagnostic that reached PHP's default error handler.
    fn last_error(&self) -> Value {
        Value::Null
    }

    /// Reset the request's last diagnostic.
    fn clear_last_error(&mut self) {}

    /// Look up a global constant by its exact name, as the INI scanner does.
    fn named_constant(&mut self, _name: &str) -> Option<Value> {
        None
    }

    /// Try the native overload of an operator, for built-ins such as pow().
    fn native_binary(
        &mut self,
        _operator: crate::NativeBinaryOp,
        _left: &Value,
        _right: &Value,
    ) -> Result<Option<Value>, NativeError> {
        Ok(None)
    }

    /// Where warnings, notices and deprecations go.
    fn diagnostics(&mut self) -> &mut dyn Diagnostics;

    /// Time available to wait on a child process or its pipes.
    fn process_wait_budget(&self) -> Option<std::time::Duration> {
        None
    }

    /// Include elapsed child I/O waits in the request execution budget.
    fn process_wait_elapsed(&mut self, _elapsed: std::time::Duration) {}

    /// End PHP execution after a child I/O wait uses the remaining budget.
    fn process_wait_expired(&mut self) -> NativeError {
        NativeError::error("Process wait exceeded the execution time limit")
    }

    /// Current PHP scope for diagnostics emitted by object handlers.
    fn diagnostic_function(&self) -> Option<&str> {
        None
    }

    /// Per-request storage for extensions; see [`RequestState`].
    fn state(&mut self) -> &mut RequestState;

    /// The PHP source location of the current operation.
    fn source_location(&self) -> Option<(String, usize)> {
        None
    }

    /// Looks up a request global such as `$_COOKIE`.
    fn request_global(&self, _name: &str) -> Option<Value> {
        None
    }

    /// Replaces the symbol, retaining aliases to the old variable.
    fn replace_request_global(&mut self, _name: &str, _value: Value) {}

    /// Flushes an extension after user shutdown callbacks and output finalization.
    fn register_request_finalizer(&mut self, _callback: crate::AsyncFn) {}

    /// Private per-request data owned by an extension.
    fn extension_value(&self, _key: &'static str) -> Option<Value> {
        None
    }

    fn set_extension_value(&mut self, _key: &'static str, _value: Value) {}

    /// Releases resources after output and extension finalizers have run.
    fn register_resource_finalizer(&mut self, _key: &'static str, _callback: crate::AsyncFn) {}

    /// The PHP file making this call, for include-path lookups.
    fn script_path(&self) -> Option<std::path::PathBuf> {
        None
    }

    /// The script selected by the SAPI, independent of the current included file.
    fn entry_script_path(&self) -> Option<std::path::PathBuf> {
        self.script_path()
    }

    /// Files executed by this request, in first-include order.
    fn included_files(&self) -> crate::PhpArray {
        crate::PhpArray::new()
    }

    /// The request's include path, used by the VM and by the file wrappers.
    fn include_path(&self) -> phera_value::PhpString {
        ".".into()
    }

    /// Return the current string value of a registered INI directive.
    fn ini_get(&self, _name: &[u8]) -> Option<PhpString> {
        None
    }

    /// A registered INI string before PHP's public ini_get() maps null to empty.
    fn ini_value(&self, name: &[u8]) -> Option<PhpString> {
        self.ini_get(name)
    }

    /// Decimal precision for serialization and PHP variable dumps.
    fn serialization_precision(&self) -> i32 {
        self.ini_get(b"serialize_precision")
            .map_or(-1, |setting| crate::ini::integer(setting.as_bytes()) as i32)
    }

    /// Change a user-modifiable setting, returning its previous value.
    fn ini_set(
        &mut self,
        _function: &str,
        _name: &[u8],
        _value: PhpString,
    ) -> Result<Option<PhpString>, NativeError> {
        Ok(None)
    }

    /// Restore a directive to its startup value.
    fn ini_restore(&mut self, _name: &[u8]) -> Result<(), NativeError> {
        Ok(())
    }

    /// List the INI settings for a module, or every loaded module.
    fn ini_list(&mut self, _module: Option<&PhpString>, _details: bool) -> NativeResult {
        Ok(Value::Bool(false))
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

    /// The fiber on this PHP stack, or none on the request's main stack.
    fn current_fiber(&self) -> Option<Rc<Object>> {
        None
    }

    fn fiber_operation(
        &mut self,
        _object: Rc<Object>,
        _operation: crate::fiber::FiberOperation,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(std::future::ready(crate::fiber::unavailable()))
    }

    fn suspend_fiber(&mut self, _value: Option<Value>) -> LocalFuture<'_, NativeResult> {
        let error = crate::fiber::fiber_error("Cannot suspend outside of a fiber");
        Box::pin(std::future::ready(Err(error)))
    }

    fn inspect_fiber(
        &mut self,
        _object: Rc<Object>,
        _inspection: crate::fiber::FiberInspection,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(std::future::ready(crate::fiber::unavailable()))
    }

    /// The location of the first write that committed the response headers.
    fn output_origin(&self) -> Option<(&str, usize)> {
        None
    }

    /// Resolves a callable in the registering scope and keeps it until the given request phase.
    fn register_request_callback(
        &mut self,
        _phase: RequestCallback,
        _arguments: Vec<Value>,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(async { Err(NativeError::error("This context has no request lifecycle")) })
    }

    /// Registers or removes a named slot without changing its order among shutdown callbacks.
    fn named_shutdown(
        &mut self,
        _key: &'static str,
        _callback: Option<&'static str>,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(async { Err(NativeError::error("This context has no request lifecycle")) })
    }

    /// Whether an object declares this method, including inherited methods.
    fn has_method(&self, _object: &Object, _name: &str) -> bool {
        false
    }

    /// Convert a value to a PHP string. A VM context may run __toString.
    fn stringify<'a>(
        &'a mut self,
        value: &'a Value,
    ) -> LocalFuture<'a, Result<PhpString, NativeError>> {
        let result = phera_value::to_php_string(value, self.diagnostics()).map_err(Into::into);
        Box::pin(std::future::ready(result))
    }

    /// Whether an operation raised a PHP exception. Some PHP functions finish
    /// a native side effect before propagating the exception to their caller.
    fn is_exception(&self, error: &NativeError) -> bool {
        matches!(error, NativeError::Throw { .. })
    }

    /// Link a new exception to the PHP exception that was already pending.
    fn chain_exception(&mut self, error: NativeError, _previous: NativeError) -> NativeError {
        error
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

    /// Expose a native operation in the trace for the duration of its PHP callback.
    fn push_callback_frame(
        &mut self,
        _name: &'static str,
        _object: Option<Rc<Object>>,
        _args: Vec<Value>,
    ) {
    }

    /// Restore the previous trace boundary after an opted-in native operation.
    fn pop_callback_frame(&mut self) {}

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

    /// Resolve an object header, invoking autoload when the class is missing.
    fn serialization_class<'a>(
        &'a mut self,
        _name: &'a str,
    ) -> LocalFuture<'a, Result<Option<crate::serialization::SerializationClass>, NativeError>>
    {
        Box::pin(std::future::ready(Ok(None)))
    }

    /// Consult the configured fallback after ordinary object autoload fails.
    fn unserialize_class_callback<'a>(
        &'a mut self,
        _name: &'a str,
    ) -> LocalFuture<'a, Result<Option<crate::serialization::SerializationClass>, NativeError>>
    {
        Box::pin(std::future::ready(Ok(None)))
    }

    /// Resolve an enum case through the runtime's canonical case table.
    fn unserialize_enum_case<'a>(
        &'a mut self,
        _class: &'a str,
        _case: &'a str,
    ) -> LocalFuture<'a, Result<crate::serialization::EnumCaseLookup, NativeError>> {
        Box::pin(std::future::ready(Ok(
            crate::serialization::EnumCaseLookup::MissingClass,
        )))
    }

    /// Read serialization metadata from an object's linked class.
    fn serialization_forbidden(&self, _object: &Object) -> bool {
        false
    }

    /// Serialization invokes linked magic methods even if declared non-public.
    fn has_serialization_method(&self, _object: &Object, _method: &str) -> bool {
        false
    }

    fn call_serialization_method(
        &mut self,
        object: Rc<Object>,
        name: &str,
        arguments: Vec<Value>,
    ) -> LocalFuture<'_, NativeResult> {
        self.call_method(object, name, arguments)
    }

    /// Allocate with declaration defaults but without constructor side effects.
    fn create_unserialized_object(&mut self, name: &str) -> NativeResult {
        self.create_object(name, PhpArray::new())
    }

    /// Publish a table of scalar properties verified against the class's linked layout.
    fn create_plain_unserialized_object(
        &mut self,
        _class: &dyn crate::value::ClassHandle,
        _properties: PhpArray,
    ) -> Option<Rc<Object>> {
        None
    }

    /// Enumerate stored properties in declaration order, including mangled names.
    fn serialization_properties(&mut self, object: &Object) -> Result<PhpArray, NativeError> {
        self.object_properties(object, crate::PropertyPurpose::Serialize)
    }

    /// An immutable class layout that can be emitted without a property callback.
    fn plain_serialization_fields(
        &self,
        _object: &Object,
    ) -> Option<Rc<[crate::serialization::SerializedProperty]>> {
        None
    }

    fn has_uninitialized_serialized_property(&self, _object: &Object, _key: &ArrayKey) -> bool {
        false
    }

    /// Map a serialized property's spelling to the destination declaration.
    fn unserialized_property_key(
        &mut self,
        _object: &Object,
        key: ArrayKey,
    ) -> Result<Option<ArrayKey>, NativeError> {
        Ok(Some(key))
    }

    fn unserialized_dynamic_property(
        &mut self,
        _object: &Object,
        _key: &ArrayKey,
    ) -> Result<(), NativeError> {
        Ok(())
    }

    /// Install raw properties without dispatching __set or checking caller visibility.
    fn finish_unserialized_object(
        &mut self,
        object: &Rc<Object>,
        properties: PhpArray,
    ) -> Result<(), NativeError> {
        for (key, value) in properties.iter() {
            object.set(key, value.clone());
        }
        Ok(())
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
    /// An object walk observes PHP state, and aliases the resulting property slots.
    fn lazy_object_for_iteration(
        &mut self,
        object: Rc<Object>,
    ) -> LocalFuture<'_, Result<Rc<Object>, crate::NativeError>> {
        Box::pin(std::future::ready(Ok(object)))
    }

    fn track_iterated_property(
        &mut self,
        _object: &Rc<Object>,
        _key: &crate::ArrayKey,
        _reference: &Rc<RefCell<Value>>,
    ) -> Result<(), crate::NativeError> {
        Ok(())
    }

    fn property_view_runs_code(&self, _object: &Object, _purpose: crate::PropertyPurpose) -> bool {
        false
    }

    /// Whether a stored property exists and is visible to the current PHP caller.
    fn has_visible_property(&self, object: &Object, name: &str) -> bool {
        object.has(&phera_value::ArrayKey::str(name))
    }

    /// Whether the caller can return a `PropertyViewRequest` and resume later.
    fn property_view_can_suspend(
        &self,
        _object: &Object,
        _purpose: crate::PropertyPurpose,
    ) -> bool {
        false
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

    /// Performs a generator operation that can finish now.
    fn generator_ready(
        &mut self,
        _object: &Object,
        _operation: &GeneratorOperation,
    ) -> Option<NativeResult> {
        None
    }

    /// Runs a generator method or iteration step on the VM.
    fn generator_operation(
        &mut self,
        _object: Rc<Object>,
        _operation: GeneratorOperation,
    ) -> crate::traversal::StepFuture<'_> {
        crate::traversal::StepFuture::ready(Err(NativeError::error(
            "Cannot resume a Generator without a runtime",
        )))
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

    /// Invoke an allocated object's constructor in its declaring scope.
    /// Factories may call this after hydrating properties, including for private constructors.
    fn call_hydration_constructor(
        &mut self,
        _object: Rc<Object>,
        _arguments: &PhpArray,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(std::future::ready(Ok(Value::Null)))
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
