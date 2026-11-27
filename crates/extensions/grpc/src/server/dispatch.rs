//! The worker: runs the PHP handler of each queued call and writes the
//! response onto the call's HTTP/2 stream.
//!
//! A handler gets the request (a decoded array, or the message bytes when
//! the route has no request type) and a `Phera\Grpc\Context`:
//!
//! ```php
//! fn (array|string $request, Context $ctx): array|string|null           // unary
//! fn (array|string $request, Context $ctx): iterable                    // server streaming
//! fn (array $requests, Context $ctx): array|string|null                 // client streaming
//! ```
//!
//! A `StatusException` becomes its status, message and trailers; any other
//! exception becomes UNKNOWN with the exception's message. `exit()` stops
//! the server.

use std::rc::Rc;

use bytes::Bytes;
use http::HeaderMap;
use phera_extension_api::traversal::{self, IteratorStep};
use phera_extension_api::value::to_bool;
use phera_extension_api::{Context, NativeError, PhpArray, Value};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use super::connection::Call;
use super::response::Responder;
use super::{Route, RouteKind, ServerState};
use crate::context::{self as grpc_context, CallState};
use crate::framing;
use crate::headers;
use crate::protobuf::{self, Schema};
use crate::status::{self, DEADLINE_EXCEEDED, INTERNAL, OK, Status};

pub async fn work(
    context: &mut dyn Context,
    routes: &[Route],
    schema: Option<&Schema>,
    mut queue: mpsc::UnboundedReceiver<Call>,
    state: &ServerState,
    shutdown: &watch::Sender<bool>,
) -> Result<(), NativeError> {
    while let Some(call) = queue.recv().await {
        handle(context, &routes[call.route], schema, call).await?;
        if state.stop.get() {
            shutdown.send_replace(true);
        }
    }
    Ok(())
}

/// How a call went wrong.
enum Failure {
    /// The handler threw, or PHP is unwinding (`exit()`).
    Php(NativeError),
    Status(Status),
    /// The client reset the stream or went away.
    Gone,
}

async fn handle(
    context: &mut dyn Context,
    route: &Route,
    schema: Option<&Schema>,
    call: Call,
) -> Result<(), NativeError> {
    let Call {
        messages,
        headers: request_headers,
        deadline,
        peer,
        mut responder,
        ..
    } = call;
    if deadline.is_some_and(|deadline| deadline <= Instant::now()) {
        finish(
            responder,
            deadline_exceeded(),
            HeaderMap::new(),
            HeaderMap::new(),
        );
        return Ok(());
    }
    let call_state = Rc::new(CallState {
        method: route.path.clone(),
        request_headers,
        deadline: deadline.map(Instant::into_std),
        peer: Some(peer),
        response_headers: Default::default(),
        trailers: Default::default(),
        headers_sent: Default::default(),
    });
    let outcome = match request_value(route, schema, &messages) {
        Ok(request) => {
            run(
                context,
                route,
                schema,
                request,
                &call_state,
                deadline,
                &mut responder,
            )
            .await
        }
        Err(status) => Err(Failure::Status(status)),
    };
    let mut trailers = call_state.trailers.take();
    let status = match outcome {
        Ok(()) => Status::new(OK, ""),
        Err(Failure::Status(status)) => status,
        Err(Failure::Gone) => {
            responder.reset();
            return Ok(());
        }
        Err(Failure::Php(error)) => match thrown_status(context, &route.path, error) {
            Ok((status, metadata)) => {
                if let Err(error) = headers::append(&mut trailers, &metadata) {
                    eprintln!(
                        "Phera\\Grpc\\Server: {}: trailer metadata dropped: {error}",
                        route.path
                    );
                }
                status
            }
            Err(error) => {
                responder.reset();
                return Err(error);
            }
        },
    };
    let response_headers = call_state.response_headers.take();
    finish(responder, status, trailers, response_headers);
    Ok(())
}

fn finish(responder: Responder, status: Status, trailers: HeaderMap, headers: HeaderMap) {
    // A client that is gone cannot be told anything.
    let _ = responder.finish(&status, trailers, headers);
}

fn deadline_exceeded() -> Status {
    Status::new(DEADLINE_EXCEEDED, "Deadline Exceeded")
}

async fn run(
    context: &mut dyn Context,
    route: &Route,
    schema: Option<&Schema>,
    request: Value,
    call_state: &Rc<CallState>,
    deadline: Option<Instant>,
    responder: &mut Responder,
) -> Result<(), Failure> {
    let ctx = grpc_context::create(context, call_state.clone()).map_err(Failure::Php)?;
    let result = context
        .call(&route.handler, vec![request, ctx])
        .await
        .map_err(Failure::Php)?;
    if route.kind != RouteKind::ServerStream {
        let frame = response_frame(route, schema, &result, responder.encoding)?;
        if deadline.is_some_and(|deadline| deadline <= Instant::now()) {
            return Err(Failure::Status(deadline_exceeded()));
        }
        return send(responder, call_state, frame).await;
    }
    match result {
        Value::Null => Ok(()),
        Value::Array(items) => {
            for item in items.values() {
                let frame = response_frame(route, schema, &item.deref(), responder.encoding)?;
                send(responder, call_state, frame).await?;
            }
            Ok(())
        }
        // A Generator, or any other Traversable, is consumed one message at
        // a time: each is on the wire before the next is produced.
        Value::Object(object) if object.class.is_traversable() => {
            let mut cursor = context.traverse(object).await.map_err(Failure::Php)?;
            let cursor = cursor.as_mut();
            traversal::step(cursor, context, IteratorStep::Rewind)
                .await
                .map_err(Failure::Php)?;
            loop {
                let valid = traversal::step(cursor, context, IteratorStep::Valid)
                    .await
                    .map_err(Failure::Php)?;
                if !to_bool(&valid) {
                    break;
                }
                let item = traversal::step(cursor, context, IteratorStep::Current)
                    .await
                    .map_err(Failure::Php)?;
                let frame = response_frame(route, schema, &item.deref(), responder.encoding)?;
                send(responder, call_state, frame).await?;
                traversal::step(cursor, context, IteratorStep::Advance)
                    .await
                    .map_err(Failure::Php)?;
            }
            Ok(())
        }
        other => Err(Failure::Status(Status::new(
            INTERNAL,
            format!(
                "a server-streaming handler must return an array or an iterable, got {}",
                other.type_name()
            ),
        ))),
    }
}

async fn send(
    responder: &mut Responder,
    call_state: &CallState,
    frame: Bytes,
) -> Result<(), Failure> {
    if !responder.started() {
        call_state.headers_sent.set(true);
        responder
            .start(call_state.response_headers.take())
            .map_err(|_| Failure::Gone)?;
    }
    responder.send(frame).await.map_err(|_| Failure::Gone)
}

/// The handler's argument: one message, or all of a client stream's.
fn request_value(
    route: &Route,
    schema: Option<&Schema>,
    messages: &[Bytes],
) -> Result<Value, Status> {
    let one = |bytes: &Bytes| match (route.request, schema) {
        (Some(id), Some(schema)) => protobuf::decode(schema, id, bytes)
            .map(Value::from)
            .map_err(|error| Status::new(INTERNAL, format!("invalid request message: {error}"))),
        _ => Ok(Value::from(&bytes[..])),
    };
    if route.kind == RouteKind::ClientStream {
        let values = messages
            .iter()
            .map(one)
            .collect::<Result<Vec<Value>, Status>>()?;
        let len = values.len();
        return Ok(Value::from(PhpArray::from_values(values, len)));
    }
    one(&messages[0])
}

/// A handler's return value as one framed message.
fn response_frame(
    route: &Route,
    schema: Option<&Schema>,
    value: &Value,
    encoding: framing::Encoding,
) -> Result<Bytes, Failure> {
    let encode = |array: &PhpArray| match (route.response, schema) {
        (Some(id), Some(schema)) => {
            let mut frame = framing::begin();
            protobuf::encode(schema, id, array, &mut frame).map_err(|error| {
                Failure::Status(Status::new(INTERNAL, format!("invalid response: {error}")))
            })?;
            Ok(framing::finish(frame, encoding))
        }
        _ => Err(Failure::Status(Status::new(
            INTERNAL,
            "the handler returned an array, but the method has no response type",
        ))),
    };
    match value {
        Value::Null => Ok(framing::frame(b"", encoding)),
        Value::Array(array) => encode(array),
        Value::Object(object) if route.response.is_some() => encode(&object.to_array()),
        other => match other.as_str_bytes() {
            Some(bytes) => Ok(framing::frame(bytes, encoding)),
            None => Err(Failure::Status(Status::new(
                INTERNAL,
                format!(
                    "a handler must return a string or an array, got {}",
                    other.type_name()
                ),
            ))),
        },
    }
}

/// The status for a handler that threw. Anything that is not an exception
/// (`exit()`) is handed back to stop the server.
fn thrown_status(
    context: &mut dyn Context,
    method: &str,
    error: NativeError,
) -> Result<(Status, PhpArray), NativeError> {
    if let Some(exception) = context.thrown_object(&error) {
        let (status, metadata) = status::from_exception(&exception);
        if !exception.class.is_instance_of(status::EXCEPTION_CLASS) {
            eprintln!(
                "Phera\\Grpc\\Server: {method}: uncaught {}: {}",
                exception.class.name(),
                status.message
            );
        }
        return Ok((status, metadata));
    }
    match error.into_exception_parts() {
        Ok((class, message, _, _)) => {
            eprintln!("Phera\\Grpc\\Server: {method}: uncaught {class}: {message}");
            Ok((Status::new(status::UNKNOWN, message), PhpArray::new()))
        }
        Err(error) => Err(error),
    }
}
