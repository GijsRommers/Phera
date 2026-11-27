//! Native extensions use Context; callbacks are bounded independently of PHP frames.

use phera_extension_api::{
    CompileDiagnostic, CompileDiagnosticKind, CompileDiagnosticSite, Context, LocalFuture,
    NativeError, NativeResult, RequestCallback, RequestState,
};
use phera_value::{Diagnostics, Value};

use crate::Vm;

impl Diagnostics for Vm {
    fn float_precision(&self) -> i32 {
        Vm::float_precision(self)
    }

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
    fn break_into_debugger(&mut self) -> bool {
        Vm::break_into_debugger(self)
    }

    fn debugger_output(&mut self, stderr: bool, bytes: &[u8]) -> bool {
        self.output_writer.debugger.as_ref().is_none_or(|debugger| {
            let stream = if stderr {
                crate::debugger::Stream::Stderr
            } else {
                crate::debugger::Stream::Stdout
            };
            debugger.buffer(stream, bytes)
        })
    }

    fn replace_caller_variable(&mut self, name: &str, value: Value) -> Result<(), NativeError> {
        if self.current().is_none() {
            self.set_global(name, value);
            return Ok(());
        }
        let local = self.named_slot(name)?;
        self.set(local, value);
        Ok(())
    }

    fn is_snapshot_boot(&self) -> bool {
        self.snapshot.mode == crate::SnapshotMode::Boot
    }

    fn set_protocol_property(
        &mut self,
        object: &phera_value::Object,
        name: &str,
        value: Value,
    ) -> Result<Option<phera_value::ArrayKey>, NativeError> {
        let property = object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
            .and_then(|class| {
                class.defaults.iter().rev().find(|property| {
                    property.name == name
                        && (property.visibility != phera_bytecode::Visibility::Private
                            || property.declaring == class.name)
                })
            });
        let key = property.map_or_else(|| phera_value::ArrayKey::str(name), |p| p.key.clone());
        if property.is_none() && object.get(&key).is_none() {
            return Ok(None);
        }
        let value = if let Some(property) = property
            && let Some(ty) = &property.ty
        {
            self.coerce_property_in_mode(value, property, ty, Some(true))?
        } else {
            value
        };
        crate::property_storage::write(object, key.clone(), value);
        Ok(Some(key))
    }

    fn extension_value(&self, key: &'static str) -> Option<Value> {
        self.extension_values.get(key).cloned()
    }

    fn set_extension_value(&mut self, key: &'static str, value: Value) {
        self.extension_values.insert(key, value);
    }

    fn reflection(
        &mut self,
    ) -> Option<&mut dyn phera_extension_api::reflection::ReflectionContext> {
        Some(self)
    }

    fn write(&mut self, bytes: &[u8]) {
        self.out(bytes)
    }

    fn write_chunk(&mut self, bytes: &[u8]) -> Option<LocalFuture<'_, Result<(), NativeError>>> {
        self.out(bytes);
        if self.output_writer.response_pending
            || (self.headers_pending() && self.output_writer.flushes_writes())
        {
            Some(Box::pin(self.drain_output()))
        } else {
            None
        }
    }

    fn connection_status(&self) -> i64 {
        i64::from(self.client_aborted()) | (i64::from(self.timed_out()) << 1)
    }

    fn finish_response(&mut self, function: &'static str) -> LocalFuture<'_, NativeResult> {
        Box::pin(Vm::finish_response_early(self, function))
    }

    fn pending_output_diagnostics(
        &mut self,
    ) -> Option<LocalFuture<'_, phera_extension_api::OutputDiagnostics>> {
        if self.error_handlers.pending.is_empty() {
            None
        } else {
            Some(self.drain_output_diagnostics())
        }
    }
    fn backtrace(&mut self, objects: bool, arguments: bool, limit: i32) -> Value {
        Value::from(self.capture_trace(phera_value::PhpArray::new(), objects, arguments, limit))
    }

    fn print_backtrace(&mut self, trace: &Value) {
        let text = crate::exception_rendering::render_backtrace(
            trace,
            self.error_handlers.trace_string_limit,
        );
        self.out(text.as_bytes());
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

    fn current_fiber(&self) -> Option<std::rc::Rc<phera_value::Object>> {
        Vm::current_fiber(self)
    }

    fn fiber_operation(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
        operation: phera_extension_api::fiber::FiberOperation,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(Vm::fiber_operation(self, object, operation))
    }

    fn suspend_fiber(&mut self, value: Option<Value>) -> LocalFuture<'_, NativeResult> {
        Box::pin(self.fiber_suspend(value))
    }

    fn inspect_fiber(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
        inspection: phera_extension_api::fiber::FiberInspection,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(self.fiber_inspect(object, inspection))
    }

    fn iconv_converter(&self) -> Option<phera_extension_api::IconvConverter> {
        self.registry.iconv_converter()
    }

    fn ini_get(&self, name: &[u8]) -> Option<phera_value::PhpString> {
        Vm::ini_get(self, name)
    }

    fn serialization_precision(&self) -> i32 {
        Vm::serialization_precision(self)
    }

    fn ini_value(&self, name: &[u8]) -> Option<phera_value::PhpString> {
        Vm::ini_value(self, name)
    }

    fn ini_set(
        &mut self,
        function: &str,
        name: &[u8],
        value: phera_value::PhpString,
    ) -> Result<Option<phera_value::PhpString>, phera_extension_api::NativeError> {
        Vm::ini_set(self, function, name, value)
    }

    fn ini_restore(&mut self, name: &[u8]) -> Result<(), phera_extension_api::NativeError> {
        Vm::ini_restore(self, name)
    }

    fn ini_list(&mut self, module: Option<&phera_value::PhpString>, details: bool) -> NativeResult {
        Vm::ini_list(self, module, details)
    }

    fn included_files(&self) -> phera_value::PhpArray {
        self.include_state
            .files()
            .enumerate()
            .map(|(index, file)| {
                (
                    phera_value::ArrayKey::Int(index as i64),
                    Value::from(file.as_str()),
                )
            })
            .collect()
    }

    fn include_path(&self) -> phera_value::PhpString {
        Vm::include_path(self)
    }

    fn set_include_wrappers(&mut self, enabled: bool) {
        self.include_state.stream_wrappers = enabled;
    }

    fn source_location(&self) -> Option<(String, usize)> {
        let location = self.location();
        Some((location.file, location.line))
    }

    fn request_global(&self, name: &str) -> Option<Value> {
        self.request_global_value(name).cloned()
    }

    fn replace_request_global(&mut self, name: &str, value: Value) {
        self.publish_global(name.into(), value);
    }

    fn register_request_finalizer(&mut self, callback: phera_extension_api::AsyncFn) {
        self.request_callbacks.output.set_finalizer(callback);
    }

    fn register_resource_finalizer(
        &mut self,
        key: &'static str,
        callback: phera_extension_api::AsyncFn,
    ) {
        self.state_mut()
            .get_or_default::<crate::request_callbacks::ResourceFinalizers>()
            .register(key, callback);
    }

    fn resource_release(
        &self,
        kind: &'static str,
        id: i64,
    ) -> Option<phera_value::ResourceRelease> {
        Some(phera_value::ResourceRelease::new(
            &self.object_ids,
            kind,
            id,
        ))
    }

    fn script_path(&self) -> Option<std::path::PathBuf> {
        Some(self.location().file.into())
    }

    fn last_error(&self) -> Value {
        self.error_handlers
            .last_error
            .as_ref()
            .map_or(Value::Null, |error| {
                error.to_value(self.error_handlers.fatal_trace.as_ref())
            })
    }

    fn clear_last_error(&mut self) {
        self.error_handlers.last_error = None;
        self.error_handlers.fatal_trace = None;
    }

    fn named_constant(&mut self, name: &str) -> Option<Value> {
        let constant = self.constants.get(name)?.clone();
        Some(self.report_global_constant(constant))
    }

    fn entry_script_path(&self) -> Option<std::path::PathBuf> {
        let main = self.include_state.entry_script()?;
        Some(self.code.known_unit(main)?.file.as_str().into())
    }

    fn process_wait_budget(&self) -> Option<std::time::Duration> {
        Vm::process_wait_budget(self)
    }

    fn process_wait_elapsed(&mut self, elapsed: std::time::Duration) {
        Vm::process_wait_elapsed(self, elapsed);
    }

    fn process_wait_expired(&mut self) -> NativeError {
        Vm::process_wait_expired(self)
    }

    fn diagnostics(&mut self) -> &mut dyn Diagnostics {
        self
    }

    fn named_shutdown(
        &mut self,
        key: &'static str,
        callback: Option<&'static str>,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(self.register_named_shutdown(key, callback))
    }

    fn has_method(&self, object: &phera_value::Object, name: &str) -> bool {
        self.classes
            .for_object(object)
            .is_some_and(|class| class.methods.contains_key(&name.to_ascii_lowercase()))
    }

    fn warning_bytes(&mut self, message: &[u8]) {
        self.diagnostic_bytes("Warning", message);
    }

    fn native_call_error(
        &mut self,
        name: &str,
        arguments: &[Value],
        error: NativeError,
    ) -> NativeError {
        if matches!(error, NativeError::Throw { .. }) {
            self.failed_native = Some(self.native_function_trace(name, arguments));
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

    fn thrown_object(&self, error: &NativeError) -> Option<std::rc::Rc<phera_value::Object>> {
        match error {
            NativeError::Unwind(payload) => payload
                .0
                .downcast_ref::<crate::exceptions::Thrown>()
                .map(|thrown| thrown.object.clone()),
            NativeError::Throw { .. } => None,
        }
    }

    fn native_binary(
        &mut self,
        operator: phera_extension_api::NativeBinaryOp,
        left: &Value,
        right: &Value,
    ) -> Result<Option<Value>, NativeError> {
        Vm::native_binary(self, operator, left, right)
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

    fn lazy_object_for_iteration(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
    ) -> LocalFuture<'_, Result<std::rc::Rc<phera_value::Object>, NativeError>> {
        Box::pin(self.initialize_lazy_via_callback(object))
    }

    fn track_iterated_property(
        &mut self,
        object: &std::rc::Rc<phera_value::Object>,
        key: &phera_value::ArrayKey,
        reference: &std::rc::Rc<std::cell::RefCell<Value>>,
    ) -> Result<(), NativeError> {
        crate::reference_holders::ReferenceHolders::track_object(
            &self.property_references,
            object,
            key,
            reference,
        )
    }

    fn property_view_runs_code(
        &self,
        object: &phera_value::Object,
        purpose: phera_extension_api::PropertyPurpose,
    ) -> bool {
        match purpose {
            phera_extension_api::PropertyPurpose::Debug => self.has_debug_info(object),
            phera_extension_api::PropertyPurpose::Json => {
                object.lazy().is_some() || self.hooked_property_class(object).is_some()
            }
            phera_extension_api::PropertyPurpose::Serialize => lazy_serialization_view(object),
            _ => false,
        }
    }

    fn property_view_can_suspend(
        &self,
        object: &phera_value::Object,
        purpose: phera_extension_api::PropertyPurpose,
    ) -> bool {
        self.native_view_depth == Some(self.call_stack.depth())
            && match purpose {
                phera_extension_api::PropertyPurpose::Json
                | phera_extension_api::PropertyPurpose::Export => {
                    object.lazy().is_some() || self.hooked_property_class(object).is_some()
                }
                phera_extension_api::PropertyPurpose::Serialize => lazy_serialization_view(object),
                _ => false,
            }
    }

    fn has_visible_property(&self, object: &phera_value::Object, name: &str) -> bool {
        self.property_key(object, name)
            .is_ok_and(|key| object.has(&key))
    }

    fn property_view(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
        purpose: phera_extension_api::PropertyPurpose,
        function: &'static str,
        arguments: &[Value],
    ) -> LocalFuture<'_, Result<phera_extension_api::PropertyView, NativeError>> {
        use phera_extension_api::{PropertyPurpose, PropertyView};
        if (matches!(purpose, PropertyPurpose::Json | PropertyPurpose::Export)
            && object.lazy().is_some())
            || (purpose == PropertyPurpose::Serialize && lazy_serialization_view(&object))
        {
            let native = crate::error_traces::NativeTrace::new(function, arguments);
            return Box::pin(async move {
                let object = self.initialize_lazy_via_callback(object).await?;
                let properties = if purpose == PropertyPurpose::Serialize {
                    self.ordered_serialization_properties(&object)?
                } else if let Some(class) = self.hooked_property_class(&object) {
                    self.enumerate_hooked_native(object, class, purpose, native)
                        .await?
                } else {
                    self.object_properties(&object, purpose)?
                };
                Ok(PropertyView::Snapshot(std::rc::Rc::new(properties)))
            });
        }
        if purpose == phera_extension_api::PropertyPurpose::Debug
            && let Some(instance) = object.lazy_instance()
        {
            let mut properties = phera_value::PhpArray::new();
            properties.insert(
                phera_value::ArrayKey::str("instance"),
                Value::Object(instance),
            );
            return Box::pin(std::future::ready(Ok(
                phera_extension_api::PropertyView::Snapshot(std::rc::Rc::new(properties)),
            )));
        }
        if purpose == phera_extension_api::PropertyPurpose::Debug && self.has_debug_info(&object) {
            return self.debug_properties(object, function, arguments);
        }
        if matches!(
            purpose,
            phera_extension_api::PropertyPurpose::Json
                | phera_extension_api::PropertyPurpose::Export
        ) && let Some(class) = self.hooked_property_class(&object)
        {
            let native = crate::error_traces::NativeTrace::new(function, arguments);
            return Box::pin(async move {
                let properties = self
                    .enumerate_hooked_native(object, class, purpose, native)
                    .await?;
                Ok(phera_extension_api::PropertyView::Snapshot(
                    std::rc::Rc::new(properties),
                ))
            });
        }
        if function == "var_dump"
            && let Some(class) = self.hooked_property_class(&object).or_else(|| {
                self.classes.for_object(&object).filter(|class| {
                    // A native dump projection (for example DateTime's) exposes its
                    // own entries. Exception uses the ordinary declared property table.
                    class
                        .hooks
                        .as_ref()
                        .is_none_or(|hooks| hooks.properties.is_none())
                        && class.defaults.iter().any(|property| property.ty.is_some())
                })
            })
        {
            let view = self.dump_hooked_slots(&object, &class);
            return Box::pin(std::future::ready(Ok(view)));
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
            message: diagnostic.message.as_str().into(),
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
            self.error_handlers.fatal_trace = None;
            self.report_event(event);
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

    fn forward_static_callback(
        &mut self,
        function: &str,
        callable: &Value,
        arguments: &phera_value::PhpArray,
    ) -> LocalFuture<'_, NativeResult> {
        use crate::calls::call_state::CallTarget;

        let call = self
            .current()
            .and_then(|frame| frame.called())
            .ok_or_else(|| {
                NativeError::error(format!(
                    "Cannot call {function}() when no class scope is active"
                ))
            });
        let resolved = call.and_then(|called| {
            let mut callee = self.resolve_callable(callable)?;
            match &mut callee.target {
                CallTarget::Method { context, .. }
                    if context.receiver.is_none() && called.is_a(&context.called.name) =>
                {
                    context.called = called;
                }
                CallTarget::Magic(magic)
                    if magic.context.receiver.is_none()
                        && called.is_a(&magic.context.called.name) =>
                {
                    std::rc::Rc::make_mut(magic).context.called = called;
                }
                _ => {}
            }
            Ok(callee)
        });
        match resolved {
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

    fn push_callback_frame(
        &mut self,
        name: &'static str,
        object: Option<std::rc::Rc<phera_value::Object>>,
        arguments: Vec<Value>,
    ) {
        use crate::error_traces::{NativeCaller, NativeTrace};
        let native = match object {
            Some(object) => NativeCaller::Method {
                name,
                object,
                arguments,
                declaration: None,
            },
            None => NativeCaller::Saved(NativeTrace::new(name, &arguments)),
        };
        self.push_native_callback(native);
    }

    fn pop_callback_frame(&mut self) {
        self.native_callback_sites.pop();
    }

    fn call_protected_method(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
        name: &str,
        arguments: Vec<Value>,
    ) -> LocalFuture<'_, NativeResult> {
        match self.protocol_method_in_scope(object, name, phera_bytecode::Visibility::Protected) {
            Ok(callee) => Box::pin(self.invoke_resolved_callable(callee, arguments)),
            Err(error) => Box::pin(std::future::ready(Err(error))),
        }
    }

    fn lookup_class<'a>(
        &'a mut self,
        name: &'a str,
    ) -> LocalFuture<
        'a,
        Result<Option<std::rc::Rc<dyn phera_value::ClassHandle>>, phera_extension_api::NativeError>,
    > {
        Box::pin(async move {
            Ok(Vm::lookup_class(self, name)
                .await?
                .map(|class| class as std::rc::Rc<dyn phera_value::ClassHandle>))
        })
    }

    fn is_user_method(&self, object: &phera_value::Object, name: &str) -> bool {
        object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
            .filter(|class| class.origin == crate::classes::class::ClassOrigin::User)
            .and_then(|class| class.methods.get(&name.to_ascii_lowercase()))
            .is_some_and(|method| matches!(method.body, crate::classes::MethodBody::User(_)))
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
                    &object,
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
                .as_ref()
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

    fn create_plain_unserialized_object(
        &mut self,
        class: &dyn phera_value::ClassHandle,
        properties: phera_value::PhpArray,
    ) -> Option<std::rc::Rc<phera_value::Object>> {
        let class = class.as_any().downcast_ref::<crate::classes::Class>()?;
        let class = class.identity.upgrade()?;
        let mut table = phera_value::Properties::for_class(&*class);
        table.replace_all(properties);
        Some(self.classes.allocate_with_properties(class, table))
    }

    fn serialization_properties(
        &mut self,
        object: &phera_value::Object,
    ) -> Result<phera_value::PhpArray, NativeError> {
        self.ordered_serialization_properties(object)
    }

    fn plain_serialization_fields(
        &self,
        object: &phera_value::Object,
    ) -> Option<std::rc::Rc<[phera_extension_api::serialization::SerializedProperty]>> {
        if object.lazy().is_some() {
            return None;
        }
        let class = object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()?;
        crate::serialization::layout::fields(class)
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
        if purpose == phera_extension_api::PropertyPurpose::ArrayCast
            && object.lazy_instance().is_some()
        {
            return Ok(object.lazy_properties());
        }
        if purpose == phera_extension_api::PropertyPurpose::ArrayCast
            && let Some(class) = self.hooked_property_class(object)
        {
            return Ok(self.raw_hooked_slots(object, &class));
        }
        let projection = object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
            .and_then(|class| class.hooks.as_ref()?.properties);
        match projection {
            Some(project) => project(self, object, purpose),
            None if purpose == phera_extension_api::PropertyPurpose::ArrayCast => {
                Ok(object.to_cast_array())
            }
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
            cursor: Some(cursor),
            initialized: false,
        }));
        Ok(value)
    }

    fn call_hydration_constructor(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
        arguments: &phera_value::PhpArray,
    ) -> LocalFuture<'_, NativeResult> {
        let callee = self
            .classes
            .for_object(&object)
            .and_then(|class| class.constructor.clone().map(|method| (class, method)));
        let Some((class, method)) = callee else {
            return Box::pin(std::future::ready(Ok(Value::Null)));
        };
        let Some(scope) = self.classes.get(&method.declaring) else {
            return Box::pin(std::future::ready(Err(internal_error!(
                "hydration constructor scope"
            ))));
        };
        let callee = self.reflection_callee(class, scope, method, Some(object));
        self.invoke_array_callback(callee, arguments)
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

    fn exit_script(&mut self) -> NativeError {
        self.release_frames_for_exit();
        NativeError::Unwind(phera_extension_api::Unwind(Box::new(
            crate::request_end::Exit,
        )))
    }

    fn report_unhandled(&mut self, error: NativeError) -> Result<(), NativeError> {
        if matches!(&error, NativeError::Unwind(payload) if payload.0.is::<crate::request_end::Exit>())
        {
            return Err(error);
        }
        // Reporting sets the exit status of a script that ends; this one
        // goes on, so its status stays what it was.
        let status = self.status;
        self.restore_silence(0);
        if let NativeError::Unwind(payload) = self.materialize_error(error) {
            match payload.0.downcast::<crate::last_error::FatalDiagnostic>() {
                Ok(diagnostic) => self.report_prepared_fatal(*diagnostic),
                Err(payload) => {
                    if let Ok(thrown) = payload.downcast::<crate::exceptions::Thrown>() {
                        self.report_throwable(&thrown.object);
                    }
                }
            }
        }
        self.status = status;
        Ok(())
    }

    fn websocket_sink(
        &self,
    ) -> Option<&std::sync::Arc<dyn phera_extension_api::websocket::WebSocketSink>> {
        self.websocket_sink_ref()
    }

    fn is_websocket_event(&self) -> bool {
        self.is_websocket_event_flag()
    }

    fn set_websocket_resolver(&mut self, resolver: Value) -> Result<(), NativeError> {
        self.store_websocket_resolver(resolver)
    }

    fn verify_websocket_handler<'a>(
        &'a mut self,
        handler: &'a str,
    ) -> LocalFuture<'a, Result<String, NativeError>> {
        Box::pin(self.verify_websocket_handler_class(handler))
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
        if class.name == "Generator" {
            return Err(NativeError::error(
                crate::generators::class::INSTANTIATION_ERROR,
            ));
        }
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
        let object = if let Some(template) = &class.property_template {
            self.classes
                .allocate_with_properties(class.clone(), template.clone())
        } else {
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
            object
        };
        if !properties.is_empty() {
            object.update(|storage| {
                if storage.is_empty() {
                    storage.replace_all(properties);
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

fn lazy_serialization_view(object: &phera_value::Object) -> bool {
    matches!(
        object.lazy().as_deref(),
        Some(
            phera_value::LazyObject::Pending {
                skip_serialization: false,
                ..
            } | phera_value::LazyObject::Proxy(_)
        )
    )
}
