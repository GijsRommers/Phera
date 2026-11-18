use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use phera_value::{Diagnostics, Object, PhpArray, Value};

use crate::{NativeError, NativeResult, RequestState};

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
