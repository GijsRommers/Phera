//! Native extensions use Context; callbacks are bounded independently of PHP frames.

use phera_extension_api::{
    CompileDiagnostic, CompileDiagnosticKind, CompileDiagnosticSite, Context, LocalFuture,
    NativeError, NativeResult, RequestCallback, RequestState,
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
    fn reflection(
        &mut self,
    ) -> Option<&mut dyn phera_extension_api::reflection::ReflectionContext> {
        Some(self)
    }

    fn write(&mut self, bytes: &[u8]) {
        self.out(bytes)
    }

    fn output_origin(&self) -> Option<(&str, usize)> {
        let origin = self.output_writer.origin.as_ref()?;
        Some((&origin.file, origin.line))
    }

    fn register_request_callback(
        &mut self,
        phase: RequestCallback,
        arguments: Vec<Value>,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(self.register_callback(phase, arguments))
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

    fn is_exception(&self, error: &NativeError) -> bool {
        match error {
            NativeError::Throw { .. } => true,
            NativeError::Unwind(payload) => payload.0.is::<crate::exceptions::Thrown>(),
        }
    }

    fn chain_exception(&mut self, error: NativeError, previous: NativeError) -> NativeError {
        self.exception_with_previous(error, previous)
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
            self.enqueue_diagnostic(event);
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
        self.create_linked_object(class, properties)
    }

    fn validate_callback(
        &mut self,
        function: &str,
        number: usize,
        parameter: &str,
        callable: &Value,
    ) -> Result<(), phera_extension_api::NativeError> {
        self.resolve_native_callback(function, number, parameter, callable)
            .map(|_| ())
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
            Ok(callee) => Box::pin(self.invoke_resolved_callable(callee, arguments)),
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
            Ok(callee) => Box::pin(self.invoke_resolved_callable(callee, arguments)),
            Err(error) => Box::pin(std::future::ready(Err(error))),
        }
    }

    fn serialization_class<'a>(
        &'a mut self,
        name: &'a str,
    ) -> LocalFuture<
        'a,
        Result<Option<phera_extension_api::serialization::SerializationClass>, NativeError>,
    > {
        Box::pin(async move {
            Ok(self
                .lookup_class(name)
                .await?
                .as_deref()
                .map(Self::serialization_class_info))
        })
    }

    fn unserialize_class_callback<'a>(
        &'a mut self,
        name: &'a str,
    ) -> LocalFuture<
        'a,
        Result<Option<phera_extension_api::serialization::SerializationClass>, NativeError>,
    > {
        Box::pin(self.run_unserialize_class_callback(name))
    }

    fn unserialize_enum_case<'a>(
        &'a mut self,
        class: &'a str,
        case: &'a str,
    ) -> LocalFuture<'a, Result<phera_extension_api::serialization::EnumCaseLookup, NativeError>>
    {
        Box::pin(self.lookup_unserialized_enum(class, case))
    }

    fn serialization_forbidden(&self, object: &phera_value::Object) -> bool {
        self.classes
            .for_object(object)
            .is_some_and(|class| Self::class_forbids_serialization(&class))
    }

    fn has_serialization_method(&self, object: &phera_value::Object, name: &str) -> bool {
        self.classes
            .for_object(object)
            .is_some_and(|class| Self::serialization_method(&class, name).is_some())
    }

    fn call_serialization_method(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
        name: &str,
        arguments: Vec<Value>,
    ) -> LocalFuture<'_, NativeResult> {
        self.invoke_serialization_method(object, name, arguments)
    }

    fn create_unserialized_object(&mut self, name: &str) -> NativeResult {
        let class = self.factory_class(name)?;
        self.allocate_unserialized_object(class)
    }

    fn serialization_properties(
        &mut self,
        object: &phera_value::Object,
    ) -> Result<phera_value::PhpArray, NativeError> {
        self.ordered_serialization_properties(object)
    }

    fn has_uninitialized_serialized_property(
        &self,
        object: &phera_value::Object,
        key: &phera_value::ArrayKey,
    ) -> bool {
        self.classes.for_object(object).is_some_and(|class| {
            class
                .defaults
                .iter()
                .any(|prop| prop.key == *key && prop.ty.is_some())
        })
    }

    fn unserialized_property_key(
        &mut self,
        object: &phera_value::Object,
        key: phera_value::ArrayKey,
    ) -> Result<Option<phera_value::ArrayKey>, NativeError> {
        Ok(self.resolve_serialized_property(object, key))
    }

    fn unserialized_dynamic_property(
        &mut self,
        object: &phera_value::Object,
        key: &phera_value::ArrayKey,
    ) -> Result<(), NativeError> {
        self.check_serialized_dynamic_property(object, key)
    }

    fn finish_unserialized_object(
        &mut self,
        object: &std::rc::Rc<phera_value::Object>,
        properties: phera_value::PhpArray,
    ) -> Result<(), NativeError> {
        self.write_unserialized_properties(object, properties)
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

    fn diagnostic_function(&self) -> Option<&str> {
        self.current()
            .map(|frame| match frame.function().name.as_str() {
                "{main}" => "main",
                function => function,
            })
    }

    fn state(&mut self) -> &mut RequestState {
        &mut self.state
    }

    fn strict_types(&self) -> bool {
        use crate::calls::callback_binding::ArgumentOrigin;
        match self.argument_origin {
            ArgumentOrigin::Php => self.current().is_some_and(|f| f.function().strict_types),
            ArgumentOrigin::Attribute => self
                .attribute_call_site()
                .is_some_and(|site| site.strict_types),
            ArgumentOrigin::Callback => false,
        }
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
        Self::check_factory_class(&class)?;
        Ok(class)
    }

    pub(crate) fn check_factory_class(
        class: &crate::classes::Class,
    ) -> Result<(), phera_extension_api::NativeError> {
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
        if matches!(
            class.kind,
            phera_bytecode::ClassKind::Enum | phera_bytecode::ClassKind::Trait
        ) {
            return Err(NativeError::error(format!(
                "Cannot instantiate {} {}",
                class.kind.description().to_ascii_lowercase(),
                class.name
            )));
        }
        if class.is_abstract {
            return Err(phera_extension_api::NativeError::error(format!(
                "Cannot instantiate abstract class {}",
                class.name
            )));
        }
        Ok(())
    }

    /// Allocate a class the caller has already resolved and checked.
    pub(crate) fn create_linked_object(
        &mut self,
        class: std::rc::Rc<crate::classes::Class>,
        properties: phera_value::PhpArray,
    ) -> NativeResult {
        let object = self.classes.allocate(class.clone());
        if let Some(template) = &class.property_template {
            if !template.is_empty() {
                object.update(|storage| *storage = template.clone());
            }
        } else {
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
        }
        if !properties.is_empty() {
            object.update(|storage| {
                if storage.is_empty() {
                    *storage = properties;
                } else {
                    for (key, value) in properties.iter() {
                        storage.insert(key, value.clone());
                    }
                }
            });
        }
        if object.property_count() != 0 {
            self.track_object_property_references(&object)?;
        }
        if class.is_throwable {
            self.initialize_throwable(&object, crate::exceptions::TraceOrigin::Construction);
        }
        Ok(Value::Object(object))
    }

    /// Native values are consumed directly into the same slots as PHP Send instructions.
    pub(crate) fn invoke_resolved_callable(
        &mut self,
        callee: crate::calls::call_state::Callee,
        arguments: impl IntoIterator<Item = Value>,
    ) -> impl Future<Output = NativeResult> {
        self.invoke_callable_origin(
            callee,
            arguments,
            crate::calls::callback_binding::ArgumentOrigin::Callback,
        )
    }
}
