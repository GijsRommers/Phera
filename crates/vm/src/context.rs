//! Native extensions use Context; callbacks are bounded independently of PHP frames.

use phera_extension_api::{
    CompileDiagnostic, CompileDiagnosticKind, CompileDiagnosticSite, Context, LocalFuture,
    NativeError, NativeResult, RequestState,
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

    fn include_path(&self) -> phera_value::PhpString {
        Vm::include_path(self)
    }

    fn script_path(&self) -> Option<std::path::PathBuf> {
        Some(self.location().file.into())
    }

    fn diagnostics(&mut self) -> &mut dyn Diagnostics {
        self
    }

    fn native_call_error(
        &mut self,
        name: &str,
        arguments: &[Value],
        error: NativeError,
    ) -> NativeError {
        if matches!(error, NativeError::Throw { .. }) {
            self.failed_native = Some(crate::error_traces::NativeTrace::new(name, arguments));
        }
        error
    }

    fn stringify<'a>(
        &'a mut self,
        value: &'a Value,
    ) -> LocalFuture<'a, Result<phera_value::PhpString, phera_extension_api::NativeError>> {
        Box::pin(self.stringify_value(value))
    }

    fn compare<'a>(
        &'a mut self,
        left: &'a Value,
        right: &'a Value,
    ) -> LocalFuture<'a, Result<i8, phera_extension_api::NativeError>> {
        Box::pin(async move {
            let value = self
                .binary_with_strings(
                    phera_bytecode::BinaryOp::Spaceship,
                    left.deref().clone(),
                    right.deref().clone(),
                )
                .await?;
            match value {
                Value::Int(order) => Ok(order.signum() as i8),
                _ => Err(internal_error!("comparison result")),
            }
        })
    }

    fn property_view_runs_code(
        &self,
        object: &phera_value::Object,
        purpose: phera_extension_api::PropertyPurpose,
    ) -> bool {
        purpose == phera_extension_api::PropertyPurpose::Debug && self.has_debug_info(object)
    }

    fn property_view(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
        purpose: phera_extension_api::PropertyPurpose,
        function: &'static str,
        arguments: &[Value],
    ) -> LocalFuture<'_, Result<phera_extension_api::PropertyView, NativeError>> {
        if self.property_view_runs_code(&object, purpose) {
            return self.debug_properties(object, function, arguments);
        }
        let view = self.object_properties(&object, purpose).map(|properties| {
            phera_extension_api::PropertyView::Snapshot(std::rc::Rc::new(properties))
        });
        Box::pin(std::future::ready(view))
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

    fn throw_object(&mut self, object: std::rc::Rc<phera_value::Object>) -> NativeError {
        let throwable = self
            .classes
            .for_object(&object)
            .is_some_and(|class| class.is_throwable);
        if !throwable {
            return NativeError::error("Cannot throw objects that do not implement Throwable");
        }
        crate::exceptions::Thrown::from_object(object).into_error()
    }

    /// Native decoders construct linked objects without running PHP constructors.
    fn create_object(&mut self, name: &str, properties: phera_value::PhpArray) -> NativeResult {
        let class = self.factory_class(name)?;
        let object = self.classes.allocate(class.clone());
        for property in &class.defaults {
            if property.initialized {
                let value = if let Some(initializer) = &property.initializer {
                    self.evaluate_property_initializer(initializer, property)?
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

    fn validate_callback(
        &mut self,
        function: &str,
        number: usize,
        parameter: &str,
        callable: &Value,
    ) -> Result<(), phera_extension_api::NativeError> {
        self.resolve_callable(callable)
            .map(|_| ())
            .map_err(|error| {
                self.invalid_callback(function, number, parameter, callable, error, false)
            })
    }

    fn call_array(
        &mut self,
        callable: &Value,
        arguments: &phera_value::PhpArray,
    ) -> LocalFuture<'_, NativeResult> {
        match self.resolve_callable(callable) {
            Ok(callee) => self.invoke_array_callback(callee, arguments),
            Err(error) => Box::pin(std::future::ready(Err(error))),
        }
    }

    fn call_method(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
        name: &str,
        arguments: Vec<Value>,
    ) -> LocalFuture<'_, NativeResult> {
        let callee = (|| {
            let class = self
                .classes
                .for_object(&object)
                .ok_or_else(|| internal_error!("extension method receiver"))?;
            self.prepare_method(
                class,
                Some(object),
                name,
                crate::classes::Invocation::Method,
            )?;
            self.take_resolved_callable()
        })();
        match callee {
            Ok(callee) => self.invoke_resolved_callable(callee, arguments),
            Err(error) => Box::pin(std::future::ready(Err(error))),
        }
    }

    fn call_protocol_method(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
        name: &str,
        arguments: Vec<Value>,
    ) -> LocalFuture<'_, NativeResult> {
        match self.protocol_method(object, name) {
            Ok(callee) => self.invoke_resolved_callable(callee, arguments),
            Err(error) => Box::pin(std::future::ready(Err(error))),
        }
    }

    fn generator_ready(
        &mut self,
        object: &phera_value::Object,
        operation: &phera_extension_api::GeneratorOperation,
    ) -> Option<NativeResult> {
        crate::generators::local_observation(object, operation)
            .or_else(|| self.resume_generator_operation(object, operation))
    }

    fn generator_operation(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
        operation: phera_extension_api::GeneratorOperation,
    ) -> phera_extension_api::traversal::StepFuture<'_> {
        use phera_extension_api::traversal::StepFuture;
        if let Some(result) = self.generator_ready(&object, &operation) {
            return StepFuture::ready(result);
        }
        StepFuture::Pending(Box::pin(async move {
            self.call_stack.enter_callback()?;
            self.generator_traces
                .push(crate::generators::trace::Site::new(
                    self.call_stack.depth(),
                    &operation,
                ));
            let result = self.operate_generator(object, operation).await;
            if let Some(site) = self.generator_traces.pop() {
                self.failed_native = self
                    .failed_native
                    .take()
                    .or_else(|| site.failed_call(&result));
            }
            self.call_stack.leave_callback();
            result
        }))
    }

    fn called_class(&self) -> Option<&str> {
        self.native_called_class
            .as_ref()
            .map(|class| class.name.as_str())
    }

    fn object_properties(
        &mut self,
        object: &phera_value::Object,
        purpose: phera_extension_api::PropertyPurpose,
    ) -> Result<phera_value::PhpArray, phera_extension_api::NativeError> {
        let projection = object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
            .and_then(|class| class.hooks.as_ref()?.properties);
        match projection {
            Some(project) => project(self, object, purpose),
            None => Ok(object.to_array()),
        }
    }

    fn count_object(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
    ) -> LocalFuture<'_, NativeResult> {
        let count = object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
            .and_then(|class| {
                let count = class.hooks.as_ref()?.count?;
                class
                    .methods
                    .get("count")
                    .is_none_or(|method| {
                        !matches!(method.body, crate::classes::MethodBody::User(_))
                    })
                    .then_some(count)
            });
        if let Some(count) = count {
            return Box::pin(std::future::ready(count(self, &object).map(Value::Int)));
        }
        Box::pin(async move {
            let result = self
                .call_protocol_method(object, "count", Vec::new())
                .await?;
            Ok(Value::Int(phera_value::to_int(&result, self)?))
        })
    }

    fn traverse(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
    ) -> LocalFuture<'_, phera_extension_api::traversal::TraversalResult> {
        Box::pin(self.make_traversal(object, false))
    }

    fn create_iterator(
        &mut self,
        cursor: Box<dyn phera_extension_api::NativeIterator>,
    ) -> NativeResult {
        let value = self.create_object("InternalIterator", phera_value::PhpArray::new())?;
        let Value::Object(object) = &value else {
            return Err(internal_error!("InternalIterator allocation"));
        };
        *object.native.borrow_mut() = Some(Box::new(crate::internal_iterator::InternalIterator {
            cursor,
            initialized: false,
        }));
        Ok(value)
    }

    fn ensure_instantiable(&self, name: &str) -> Result<(), phera_extension_api::NativeError> {
        self.factory_class(name).map(drop)
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
    fn factory_class(
        &self,
        name: &str,
    ) -> Result<std::rc::Rc<crate::classes::Class>, phera_extension_api::NativeError> {
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
        Ok(class)
    }

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
