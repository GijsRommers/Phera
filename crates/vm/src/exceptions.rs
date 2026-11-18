//! Throwable objects are the VM's exception transport across native boundaries.
//!
//! `throw $failure` retains the same object in NativeError::Unwind. Engine and
//! native failures are materialized before frames are released, so catch and
//! uncaught reporting observe identical file, line, code and stack information.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::{NativeError, Unwind};
use phera_value::{Object, Value};

use crate::Vm;

/// Creation traces omit a failed native call; engine traces include that call itself.
#[derive(Clone, Copy)]
pub(crate) enum TraceOrigin {
    Construction,
    Engine,
}

/// Opaque native transport preserves PHP object identity during unwinding.
#[derive(Debug)]
pub(crate) struct Thrown {
    pub(crate) object: Rc<Object>,
    /// PHP return-opcode warnings escape the returning frame's own catches.
    pub(crate) skip_handlers: Option<usize>,
}

impl Thrown {
    /// Native callbacks forward this payload without constructing another exception.
    pub(crate) fn into_error(self) -> NativeError {
        NativeError::Unwind(Unwind(Box::new(self)))
    }
}

impl Vm {
    /// PHP accepts only objects implementing Throwable, including derived native classes.
    pub(crate) fn throw_operand(&mut self, source: Reg) -> Result<NativeError, NativeError> {
        let Value::Object(object) = self.read_local(source) else {
            return Err(NativeError::error("Can only throw objects"));
        };
        if !self
            .classes
            .for_object(&object)
            .is_some_and(|class| class.is_throwable)
        {
            return Err(NativeError::error(
                "Cannot throw objects that do not implement Throwable",
            ));
        }
        Ok(Thrown {
            object,
            skip_handlers: None,
        }
        .into_error())
    }

    /// Drop native diagnostic preparation state before an async method can suspend.
    pub(crate) fn prepare_native_method_arguments(
        &mut self,
        method: &crate::classes::Method,
        parameters: &[phera_extension_api::Param],
        arguments: &mut [Value],
    ) -> Result<(), NativeError> {
        let call_name = format!("{}::{}", method.declaring, method.name);
        let tracking = self.error_handlers.error.is_some() && !self.error_handlers.active;
        let previous = if tracking {
            self.error_handlers
                .native_site
                .replace(crate::error_traces::NativeTrace::new(&call_name, arguments))
        } else {
            None
        };
        let prepared = self.prepare_throwable_constructor(&call_name, parameters, arguments);
        if tracking {
            self.error_handlers.native_site = previous;
        }
        if prepared.is_err() {
            self.failed_native = Some(crate::error_traces::NativeTrace::new(&call_name, arguments));
        }
        prepared
    }

    /// Convert earlier constructor parameters before checking the narrower previous contract.
    pub(crate) fn prepare_throwable_constructor(
        &mut self,
        name: &str,
        parameters: &[phera_extension_api::Param],
        arguments: &mut [Value],
    ) -> Result<(), NativeError> {
        let previous = match name {
            "Exception::__construct" | "Error::__construct" => Some(2),
            "ErrorException::__construct" => Some(5),
            _ => None,
        };
        if let Some(index) =
            previous.filter(|index| arguments.len() > *index && arguments.len() <= parameters.len())
        {
            self.prepare_call_with_trace(name, &parameters[..index], &mut arguments[..index])?;
            self.check_throwable_constructor(name, arguments)?;
        }
        self.prepare_call_with_trace(name, parameters, arguments)
    }

    /// Native Throwable constructors require a Throwable previous value, rather than any object.
    pub(crate) fn check_throwable_constructor(
        &self,
        name: &str,
        arguments: &[Value],
    ) -> Result<(), NativeError> {
        let index = match name {
            "Exception::__construct" | "Error::__construct" => 2,
            "ErrorException::__construct" => 5,
            _ => return Ok(()),
        };
        let Some(previous) = arguments.get(index).filter(|value| !value.is_null()) else {
            return Ok(());
        };
        if matches!(previous, Value::Object(object) if self.classes.for_object(object).is_some_and(|class| class.is_throwable))
        {
            return Ok(());
        }
        let actual = match previous {
            Value::Object(object) => object.class.name().to_owned(),
            value => value.type_name(),
        };
        Err(NativeError::type_error(format!(
            "{name}(): Argument #{} ($previous) must be of type ?Throwable, {actual} given",
            index + 1
        )))
    }

    /// Creation, rather than throwing, fixes the source location and stack snapshot.
    #[cold]
    #[inline(never)]
    pub(crate) fn initialize_throwable(&mut self, object: &Rc<Object>, origin: TraceOrigin) {
        let location = self.location();
        object.set(Object::protected_key("file"), Value::from(location.file));
        object.set(
            Object::protected_key("line"),
            Value::Int(location.line as i64),
        );
        let trace = self.throwable_trace(origin);
        object.set(Object::private_key(throwable_root(object), "trace"), trace);
    }

    /// Turn engine/native failures into the same transport as an explicit PHP throw.
    #[cold]
    pub(crate) fn materialize_error(&mut self, error: NativeError) -> NativeError {
        let (class, message, code, properties) = match error.into_exception_parts() {
            Ok(parts) => parts,
            Err(error) => {
                if matches!(&error, NativeError::Unwind(payload) if payload.0.is::<Thrown>()) {
                    self.failed_native = None;
                }
                return error;
            }
        };
        let Some(linked) = self.classes.get(class) else {
            return match code {
                Value::Int(code) if properties.is_empty() => {
                    NativeError::throw_with_code(class, message, code)
                }
                _ => NativeError::error(format!(
                    "Phera internal error: native exception class {class} is not registered"
                )),
            };
        };
        let mut declared_properties = Vec::with_capacity(properties.len());
        for (name, value) in properties.iter() {
            let property = linked
                .defaults
                .iter()
                .rev()
                .find(|property| name == phera_value::ArrayKey::str(property.name.as_str()));
            let Some(property) = property else {
                let name = match name {
                    phera_value::ArrayKey::String(name) => name.to_string_lossy(),
                    phera_value::ArrayKey::Int(name) => name.to_string(),
                };
                return self.materialize_error(NativeError::error(format!(
                    "Phera internal error: native exception property \
                     {class}::${name} is not declared"
                )));
            };
            declared_properties.push((property.key.clone(), value.clone()));
        }
        let object = self.classes.allocate(linked.clone());
        for property in &linked.defaults {
            if property.initialized {
                object.set(property.key.clone(), property.value.clone());
            }
        }
        self.initialize_throwable(&object, TraceOrigin::Engine);
        object.set(Object::protected_key("message"), Value::from(message));
        object.set(Object::protected_key("code"), code);
        for (key, value) in declared_properties {
            object.set(key, value);
        }
        Thrown {
            object,
            skip_handlers: None,
        }
        .into_error()
    }

    /// Render native accessors from VM snapshots, including named arguments and escaped bytes.
    pub(crate) fn throwable_method_result(&self, name: &str, object: &Rc<Object>) -> Option<Value> {
        match name.to_ascii_lowercase().as_str() {
            "gettraceasstring" => {
                let trace = object
                    .get(&Object::private_key(throwable_root(object), "trace"))
                    .unwrap_or_default();
                Some(Value::from(crate::exception_rendering::render_trace(
                    &trace,
                    self.error_handlers.trace_string_limit,
                )))
            }
            "__tostring" => {
                let text = Value::from(crate::exception_rendering::describe_chain(
                    object,
                    self.error_handlers.trace_string_limit,
                ));
                object.set(
                    Object::private_key(throwable_root(object), "string"),
                    text.clone(),
                );
                Some(text)
            }
            _ => None,
        }
    }

    /// Stored object metadata survives complete unwinding of the original stack.
    pub(crate) fn report_throwable(&mut self, object: &Rc<Object>) {
        if object.class.name() == "ParseError" {
            let message = property_text(object, "message");
            let file = property_text(object, "file");
            let line = property_text(object, "line");
            self.out(format!("\nParse error: {message} in {file} on line {line}\n").as_bytes());
            self.status = 255;
            return;
        }
        let text = crate::exception_rendering::describe_chain(
            object,
            self.error_handlers.trace_string_limit,
        );
        let file = property_text(object, "file");
        let file = if file.is_empty() {
            "Unknown".to_owned()
        } else {
            file
        };
        let line = property_text(object, "line");
        self.out(
            format!("\nFatal error: Uncaught {text}\n  thrown in {file} on line {line}\n")
                .as_bytes(),
        );
        self.status = 255;
    }
}

/// PHP stores inherited private fields under the native root declaring them.
pub(crate) fn throwable_root(object: &Object) -> &'static str {
    if object.has(&Object::private_key("Error", "trace")) {
        "Error"
    } else {
        "Exception"
    }
}

/// Throwable diagnostic fields are scalars, so reading them never invokes user code.
fn property_text(object: &Object, name: &str) -> String {
    let value = object.get(&Object::protected_key(name)).unwrap_or_default();
    phera_value::to_php_string(&value, &mut phera_value::NoDiagnostics)
        .map(|text| text.to_string_lossy())
        .unwrap_or_default()
}
