//! Native extensions use Context; callbacks are bounded independently of PHP frames.

use phera_extension_api::{
    CompileDiagnostic, CompileDiagnosticKind, CompileDiagnosticSite, Context, LocalFuture,
    NativeResult, RequestState,
};
use phera_value::{Diagnostics, Value};

use crate::Vm;

impl Diagnostics for Vm {
    fn warning(&mut self, message: String) {
        self.diagnostic("Warning", message)
    }

    fn notice(&mut self, message: String) {
        self.diagnostic("Notice", message)
    }

    fn deprecated(&mut self, message: String) {
        self.diagnostic("Deprecated", message)
    }
}

impl Context for Vm {
    fn write(&mut self, bytes: &[u8]) {
        self.out(bytes)
    }

    fn script_path(&self) -> Option<std::path::PathBuf> {
        Some(self.location().file.into())
    }

    fn diagnostics(&mut self) -> &mut dyn Diagnostics {
        self
    }

    fn compile_diagnostic(
        &mut self,
        diagnostic: &CompileDiagnostic,
        site: CompileDiagnosticSite<'_>,
    ) {
        let caller = self.location();
        let location = match site {
            CompileDiagnosticSite::Caller => caller.clone(),
            CompileDiagnosticSite::Source(file) => crate::diagnostics::SourceLocation {
                file: file.to_owned(),
                line: diagnostic.line,
            },
        };
        let level = diagnostic.kind.error_level();
        let calls_handler =
            diagnostic.kind == CompileDiagnosticKind::Deprecated && self.handles_diagnostic(level);
        let event = crate::error_handlers::DiagnosticEvent {
            level,
            kind: diagnostic.kind.label().into(),
            message: diagnostic.message.clone(),
            location,
            native: self.error_handlers.native_site.clone(),
            trace_location: Some(caller),
        };
        // Compile warnings bypass the PHP handler. If an earlier deprecation
        // queued a callback, publish later warnings only if that callback returns.
        if calls_handler || (!self.error_handlers.active && !self.error_handlers.pending.is_empty())
        {
            self.error_handlers.pending.push(event);
        } else {
            self.report_event(&event);
        }
    }

    /// Native decoders construct linked objects without running PHP constructors.
    fn create_object(&mut self, name: &str, properties: phera_value::PhpArray) -> NativeResult {
        let class = self.classes.get(name).ok_or_else(|| {
            phera_extension_api::NativeError::error(format!("Class \"{name}\" not found"))
        })?;
        if class.origin == crate::classes::class::ClassOrigin::Closure {
            return Err(phera_extension_api::NativeError::error(
                "Instantiation of class Closure is not allowed",
            ));
        }
        if class.kind == phera_bytecode::ClassKind::Interface {
            return Err(phera_extension_api::NativeError::error(format!(
                "Cannot instantiate interface {}",
                class.name
            )));
        }
        if class.is_abstract {
            return Err(phera_extension_api::NativeError::error(format!(
                "Cannot instantiate abstract class {}",
                class.name
            )));
        }
        let object = self.classes.allocate(class.clone());
        for property in &class.defaults {
            if property.initialized {
                let value = if let Some(initializer) = &property.initializer {
                    self.evaluate_initializer(initializer, &property.declaring)?
                } else {
                    property.value.clone()
                };
                object.set(property.key.clone(), value);
            }
        }
        object.update(|storage| {
            for (key, value) in properties.iter() {
                storage.insert(key.clone(), value.clone());
            }
        });
        self.track_object_property_references(&object)?;
        if class.is_throwable {
            self.initialize_throwable(&object, crate::exceptions::TraceOrigin::Construction);
        }
        Ok(Value::Object(object))
    }

    fn state(&mut self) -> &mut RequestState {
        &mut self.state
    }

    fn strict_types(&self) -> bool {
        self.argument_origin == crate::calls::callback_binding::ArgumentOrigin::Php
            && self.current().is_some_and(|f| f.function().strict_types)
    }

    /// Native callbacks await the heap interpreter with a separate bound on Rust nesting.
    fn call<'a>(
        &'a mut self,
        callable: &'a Value,
        arguments: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            self.autoload_callable(callable).await?;
            let callee = self.resolve_callable(callable)?;
            self.invoke_resolved_callable(callee, arguments).await
        })
    }
}

impl Vm {
    /// Native values are consumed directly into the same slots as PHP Send instructions.
    pub(crate) fn invoke_resolved_callable(
        &mut self,
        callee: crate::calls::call_state::Callee,
        arguments: impl IntoIterator<Item = Value>,
    ) -> LocalFuture<'_, NativeResult> {
        self.invoke_callable_origin(
            callee,
            arguments,
            crate::calls::callback_binding::ArgumentOrigin::Callback,
        )
    }
}
