//! Runs one WebSocket lifecycle event (`onOpen`, `onMessage`, `onClose`) in an
//! isolated VM (forked from a snapshot or freshly booted in classic mode).
//!
//! Snapshotted scripts never resume bytecode past `Phera\snapshot()` for an
//! event; instead the VM resolves the `$handlerClass` via the snapshot's
//! `Phera\WebSocket\resolveUsing(...)` resolver (or `new $handlerClass()`) and
//! invokes the handler method directly.

use std::rc::Rc;
use std::sync::Arc;

use phera_bytecode::{CompiledUnit, Visibility};
use phera_extension_api::websocket::{WebSocketEvent, WebSocketEventOutcome, WebSocketSink};
use phera_extension_api::{Context, NativeError};
use phera_value::{ArrayKey, Object, PhpArray, Value};

use crate::Vm;
use crate::exceptions::Thrown;
use crate::request_end::{Exit, ScriptEnd};

const HANDLER_INTERFACE: &str = "Phera\\WebSocket\\Handler";
const CONNECTION_CLASS: &str = "Phera\\WebSocket\\Connection";
const MESSAGE_CLASS: &str = "Phera\\WebSocket\\Message";

impl Vm {
    /// Attaches the host's [`WebSocketSink`] for `Phera\WebSocket` calls.
    pub fn set_websocket_sink(&mut self, sink: Option<Arc<dyn WebSocketSink>>) {
        self.websocket_sink = sink;
    }

    pub(crate) fn websocket_sink_ref(&self) -> Option<&Arc<dyn WebSocketSink>> {
        self.websocket_sink.as_ref()
    }

    pub(crate) fn is_websocket_event_flag(&self) -> bool {
        self.websocket_is_event
    }

    pub(crate) fn store_websocket_resolver(&mut self, resolver: Value) -> Result<(), NativeError> {
        let _ = self.resolve_callable(&resolver)?;
        self.websocket_resolver = Some(resolver);
        Ok(())
    }

    /// Ensures `handler` loads and names an instantiable `Phera\WebSocket\Handler`.
    pub(crate) async fn verify_websocket_handler_class(
        &mut self,
        handler: &str,
    ) -> Result<String, NativeError> {
        let Some(class) = self.lookup_class_from(handler, None).await? else {
            return Err(NativeError::throw(
                "InvalidArgumentException",
                format!("WebSocket handler class \"{handler}\" not found"),
            ));
        };
        if let Err(err) = Self::check_factory_class(&class) {
            return Err(Self::rethrow_as_invalid_argument(err));
        }
        if !self.class_matches(&class, HANDLER_INTERFACE) {
            return Err(NativeError::throw(
                "InvalidArgumentException",
                format!("WebSocket handler \"{handler}\" must implement {HANDLER_INTERFACE}"),
            ));
        }
        Ok(class.name.clone())
    }

    fn rethrow_as_invalid_argument(error: NativeError) -> NativeError {
        match error {
            NativeError::Throw { message, .. } => {
                NativeError::throw("InvalidArgumentException", message)
            }
            other => other,
        }
    }

    /// Runs a single WebSocket lifecycle event (`Open`, `Message`, or `Close`).
    pub async fn run_websocket_event(
        &mut self,
        unit: Option<Arc<CompiledUnit>>,
        connection_id: &str,
        handler_class: &str,
        event: WebSocketEvent,
    ) -> WebSocketEventOutcome {
        self.execution_time
            .clone()
            .track(self.run_websocket_event_inner(unit, connection_id, handler_class, event))
            .await
    }

    async fn run_websocket_event_inner(
        &mut self,
        unit: Option<Arc<CompiledUnit>>,
        connection_id: &str,
        handler_class: &str,
        event: WebSocketEvent,
    ) -> WebSocketEventOutcome {
        self.websocket_is_event = true;
        if !self.is_fork()
            && let Some(unit) = unit
            && let Err(error) = self.run_classic_declarations(&unit).await
        {
            let outcome = self.finish_unhandled_websocket_error(error, false).await;
            return outcome;
        }

        let handler = match self.instantiate_websocket_handler(handler_class).await {
            Ok(handler) => handler,
            Err(error) => {
                return self.finish_unhandled_websocket_error(error, true).await;
            }
        };

        let connection = match self.build_websocket_connection(connection_id).await {
            Ok(conn) => conn,
            Err(error) => {
                return self.finish_unhandled_websocket_error(error, true).await;
            }
        };

        let allows_on_error =
            matches!(event, WebSocketEvent::Open | WebSocketEvent::Message { .. });
        let call_result = match event {
            WebSocketEvent::Open => {
                Context::call_method(self, handler.clone(), "onOpen", vec![connection.clone()])
                    .await
            }
            WebSocketEvent::Message { data, binary } => {
                match self.build_websocket_message(data, binary).await {
                    Ok(message) => {
                        Context::call_method(
                            self,
                            handler.clone(),
                            "onMessage",
                            vec![connection.clone(), message],
                        )
                        .await
                    }
                    Err(error) => Err(error),
                }
            }
            WebSocketEvent::Close { code, reason } => {
                Context::call_method(
                    self,
                    handler.clone(),
                    "onClose",
                    vec![
                        connection.clone(),
                        Value::Int(i64::from(code)),
                        Value::from(reason),
                    ],
                )
                .await
            }
        };

        let mut close_1011 = false;
        let mut end = ScriptEnd::Destructible;
        if let Err(error) = call_result
            && !is_exit(&error)
        {
            let error = self.materialize_error(error);
            let throwable = thrown_object(&error);
            end = ScriptEnd::of(&error);
            self.unwind(error);
            if allows_on_error {
                close_1011 = true;
                if let Some(throwable) = throwable
                    && let Err(on_error_err) = Context::call_method(
                        self,
                        handler,
                        "onError",
                        vec![connection, Value::Object(throwable)],
                    )
                    .await
                    && !is_exit(&on_error_err)
                {
                    let on_error_err = self.materialize_error(on_error_err);
                    if ScriptEnd::of(&on_error_err) == ScriptEnd::Fatal {
                        end = ScriptEnd::Fatal;
                    }
                    self.unwind(on_error_err);
                }
            }
        }

        self.release_frames_for_exit();
        self.end_script(end).await;
        WebSocketEventOutcome { close_1011 }
    }

    async fn run_classic_declarations(
        &mut self,
        unit: &Arc<CompiledUnit>,
    ) -> Result<(), NativeError> {
        self.load_entry_unit(unit)?;
        self.interpret(0).await.map(|_| ())
    }

    async fn finish_unhandled_websocket_error(
        &mut self,
        error: NativeError,
        close_on_error: bool,
    ) -> WebSocketEventOutcome {
        if is_exit(&error) {
            self.release_frames_for_exit();
            self.end_script(ScriptEnd::Destructible).await;
            return WebSocketEventOutcome { close_1011: false };
        }
        let error = self.materialize_error(error);
        let end = ScriptEnd::of(&error);
        self.unwind(error);
        self.release_frames_for_exit();
        self.end_script(end).await;
        WebSocketEventOutcome {
            close_1011: close_on_error,
        }
    }

    async fn instantiate_websocket_handler(
        &mut self,
        handler_class: &str,
    ) -> Result<Rc<Object>, NativeError> {
        if let Some(resolver) = self.websocket_resolver.clone() {
            self.autoload_callable(&resolver).await?;
            let callee = self.resolve_callable(&resolver)?;
            let value = self
                .invoke_resolved_callable(callee, [Value::from(handler_class)])
                .await?;
            let Value::Object(object) = value else {
                return Err(NativeError::throw(
                    "TypeError",
                    format!("WebSocket resolver for {handler_class} must return an object"),
                ));
            };
            let class = self
                .classes
                .for_object(&object)
                .ok_or_else(|| NativeError::error("WebSocket handler class missing"))?;
            if !self.class_matches(&class, HANDLER_INTERFACE) {
                return Err(NativeError::throw(
                    "TypeError",
                    format!(
                        "Resolved handler {} must implement {HANDLER_INTERFACE}",
                        class.name
                    ),
                ));
            }
            return Ok(object);
        }

        let Some(class) = self.lookup_class_from(handler_class, None).await? else {
            return Err(NativeError::throw(
                "Error",
                format!("Class \"{handler_class}\" not found"),
            ));
        };
        Self::check_factory_class(&class)?;
        if !self.class_matches(&class, HANDLER_INTERFACE) {
            return Err(NativeError::throw(
                "TypeError",
                format!(
                    "Handler \"{}\" must implement {HANDLER_INTERFACE}",
                    class.name
                ),
            ));
        }
        let Value::Object(object) = self.create_linked_object(class.clone(), PhpArray::new())?
        else {
            return Err(NativeError::error("failed to allocate WebSocket handler"));
        };
        if let Some(constructor) = class.constructor.as_ref() {
            if constructor.visibility != Visibility::Public {
                return Err(NativeError::throw(
                    "Error",
                    format!("Call to non-public {}::__construct()", class.name),
                ));
            }
            Context::call_hydration_constructor(self, object.clone(), &PhpArray::new()).await?;
        }
        Ok(object)
    }

    async fn build_websocket_connection(
        &mut self,
        connection_id: &str,
    ) -> Result<Value, NativeError> {
        let Some(class) = self.lookup_class_from(CONNECTION_CLASS, None).await? else {
            return Err(NativeError::error("Phera\\WebSocket\\Connection not found"));
        };
        let mut properties = PhpArray::new();
        properties.insert(
            Object::private_key(CONNECTION_CLASS, "id"),
            Value::from(connection_id),
        );
        self.create_linked_object(class, properties)
    }

    async fn build_websocket_message(
        &mut self,
        data: Vec<u8>,
        binary: bool,
    ) -> Result<Value, NativeError> {
        let Some(class) = self.lookup_class_from(MESSAGE_CLASS, None).await? else {
            return Err(NativeError::error("Phera\\WebSocket\\Message not found"));
        };
        let mut properties = PhpArray::new();
        properties.insert(ArrayKey::str("data"), Value::from(data));
        properties.insert(ArrayKey::str("binary"), Value::Bool(binary));
        self.create_linked_object(class, properties)
    }
}

fn is_exit(error: &NativeError) -> bool {
    matches!(error, NativeError::Unwind(payload) if payload.0.is::<Exit>())
}

fn thrown_object(error: &NativeError) -> Option<Rc<Object>> {
    match error {
        NativeError::Unwind(payload) => payload
            .0
            .downcast_ref::<Thrown>()
            .map(|thrown| thrown.object.clone()),
        NativeError::Throw { .. } => None,
    }
}
