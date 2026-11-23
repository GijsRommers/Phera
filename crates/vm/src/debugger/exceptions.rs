//! `breakpoint_set -t exception -x RuntimeException -i 1` also matches subclasses.

use phera_extension_api::{LocalFuture, NativeError};
use phera_value::{ArrayKey, Object, Value};
use quick_xml::escape::escape;

use crate::Vm;

use super::breakpoints::Kind;
use super::protocol::file_uri;
use super::session::Stop;

impl Vm {
    #[cold]
    pub(crate) fn debug_exception(&mut self, error: NativeError) -> LocalFuture<'_, NativeError> {
        Box::pin(async move {
            if self
                .debugger
                .as_ref()
                .is_none_or(|debugger| !debugger.breakpoints.has_exceptions())
            {
                return error;
            }
            let error = self.materialize_error(error);
            let NativeError::Unwind(payload) = &error else {
                return error;
            };
            let Some(thrown) = payload.0.downcast_ref::<crate::exceptions::Thrown>() else {
                return error;
            };
            let Some(class) = self.classes.for_object(&thrown.object) else {
                return error;
            };
            let location = self.location();
            let Some(debugger) = &mut self.debugger else {
                return error;
            };
            if debugger.last_exception == Some(thrown.object.id) {
                return error;
            }
            let candidates = debugger.breakpoints.candidates(|kind| match kind {
                Kind::Exception(name) => name == "*" || class.is_a(name),
                _ => false,
            });
            if candidates.is_empty() {
                return error;
            }
            debugger.last_exception = Some(thrown.object.id);
            let message = scalar(&thrown.object, "message");
            let code = scalar(&thrown.object, "code");
            debugger.stop = Some(Stop {
                location: format!(
                    r#"<xdebug:message filename="{}" lineno="{}" exception="{}" code="{}">{}</xdebug:message>"#,
                    escape(file_uri(&location.file)),
                    location.line,
                    escape(&class.name),
                    escape(code),
                    escape(message),
                ),
                line: location.line as u32,
                unconditional: false,
                candidates,
            });
            match self.debugger_commands().await {
                Ok(()) => error,
                Err(stop) => stop,
            }
        })
    }
}

fn scalar(object: &Object, name: &str) -> String {
    let value = object
        .get(&Object::protected_key(name))
        .or_else(|| object.get(&ArrayKey::str(name)));
    let value = value.as_ref().and_then(|value| value.try_deref().ok());
    if let Some(text) = value.as_deref().and_then(Value::str_ref) {
        return text.to_string_lossy();
    }
    match value.as_deref() {
        Some(Value::Int(value)) => value.to_string(),
        _ => String::new(),
    }
}
