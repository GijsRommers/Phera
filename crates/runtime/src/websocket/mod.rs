//! PHP-facing `Phera\WebSocket\*` API backed by [`phera_extension_api::websocket::WebSocketSink`].
//!
//! ```php
//! if (!Phera\WebSocket\isEvent()) {
//!     Phera\WebSocket\accept(ChatHandler::class, context: ['user' => 1], rooms: ['lobby']);
//! }
//! ```

mod connection;
pub(crate) mod context;

use phera_extension_api::websocket::WebSocketAccept;
use phera_extension_api::{
    BundledSource, ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult,
    Param, ParamType, PhpArray, Registry, Value, args, fresh_state,
};

const CONNECTION_CLASS: &str = "Phera\\WebSocket\\Connection";
const HANDLER_INTERFACE: &str = "Phera\\WebSocket\\Handler";
const MESSAGE_CLASS: &str = "Phera\\WebSocket\\Message";

const WEBSOCKET_PHP: BundledSource = BundledSource {
    file: "phera://websocket/WebSocket.php",
    source: include_str!("WebSocket.php"),
};

/// Pending upgrade parameters recorded by `Phera\WebSocket\accept()`.
#[derive(Default)]
pub(crate) struct PendingWebSocketAccept(pub(crate) Option<WebSocketAccept>);

/// Registers `Phera\WebSocket` functions, `Connection`, bundled `Handler`/`Message`, and state fork.
pub(crate) fn register(registry: &mut Registry) {
    registry.fork_state::<PendingWebSocketAccept>(fresh_state);
    registry.bundled_class(HANDLER_INTERFACE, WEBSOCKET_PHP);
    registry.bundled_class(MESSAGE_CLASS, WEBSOCKET_PHP);

    register_functions(registry);
    connection::register(registry);
}

fn register_functions(registry: &mut Registry) {
    const ACCEPT_PARAMS: &[Param] = &[
        Param::new("handler", ParamType::String),
        Param::new("context", ParamType::Array).with_default(ConstantValue::EmptyArray),
        Param::new("rooms", ParamType::Array).with_default(ConstantValue::EmptyArray),
        Param::new("subprotocol", ParamType::String)
            .nullable()
            .with_default(ConstantValue::Null),
    ];
    registry.function(FunctionEntry::asynchronous(
        "Phera\\WebSocket\\accept",
        ACCEPT_PARAMS,
        accept_fn,
    ));
    registry.describe_function("Phera\\WebSocket\\accept", "void", None);

    const BROADCAST_PARAMS: &[Param] = &[
        Param::new("room", ParamType::String),
        Param::new("data", ParamType::String),
        Param::new("binary", ParamType::Bool).with_default(ConstantValue::Bool(false)),
    ];
    registry.function(FunctionEntry::sync(
        "Phera\\WebSocket\\broadcast",
        BROADCAST_PARAMS,
        broadcast_fn,
    ));
    registry.describe_function("Phera\\WebSocket\\broadcast", "int", None);

    const SEND_PARAMS: &[Param] = &[
        Param::new("connectionId", ParamType::String),
        Param::new("data", ParamType::String),
        Param::new("binary", ParamType::Bool).with_default(ConstantValue::Bool(false)),
    ];
    registry.function(FunctionEntry::sync(
        "Phera\\WebSocket\\send",
        SEND_PARAMS,
        send_fn,
    ));
    registry.describe_function("Phera\\WebSocket\\send", "bool", None);

    const ROOM_PARAM: &[Param] = &[Param::new("room", ParamType::String)];
    registry.function(FunctionEntry::sync(
        "Phera\\WebSocket\\roomCount",
        ROOM_PARAM,
        room_count_fn,
    ));
    registry.describe_function("Phera\\WebSocket\\roomCount", "int", None);

    registry.function(FunctionEntry::sync(
        "Phera\\WebSocket\\connections",
        ROOM_PARAM,
        connections_fn,
    ));
    registry.describe_function("Phera\\WebSocket\\connections", "array", None);

    const RESOLVER_PARAM: &[Param] = &[Param::new("resolver", ParamType::Callable)];
    registry.function(FunctionEntry::sync(
        "Phera\\WebSocket\\resolveUsing",
        RESOLVER_PARAM,
        resolve_using_fn,
    ));
    registry.describe_function("Phera\\WebSocket\\resolveUsing", "void", None);

    registry.function(FunctionEntry::sync(
        "Phera\\WebSocket\\isEvent",
        &[],
        is_event_fn,
    ));
    registry.describe_function("Phera\\WebSocket\\isEvent", "bool", None);
}

pub(super) fn utf8_arg(arguments: &[Value], index: usize) -> Result<String, NativeError> {
    std::str::from_utf8(args::bytes(arguments, index))
        .map(str::to_owned)
        .map_err(|_| NativeError::throw("ValueError", "Argument must be valid UTF-8"))
}

pub(super) fn validate_outbound_payload(data: &[u8], binary: bool) -> Result<(), NativeError> {
    if !binary && std::str::from_utf8(data).is_err() {
        return Err(NativeError::throw(
            "ValueError",
            "WebSocket text message must be valid UTF-8",
        ));
    }
    Ok(())
}

fn accept_fn(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        if context.is_websocket_event() {
            return Err(NativeError::throw(
                "LogicException",
                "Cannot call Phera\\WebSocket\\accept() inside a WebSocket event handler",
            ));
        }
        if context.websocket_sink().is_none() {
            return Err(NativeError::throw(
                "Phera\\NotSupportedException",
                "Phera\\WebSocket\\accept() is only available inside phera serve",
            ));
        }

        let handler = utf8_arg(&arguments, 0)?;
        let canonical_handler = context.verify_websocket_handler(&handler).await?;

        let context_bytes = match args::array(&arguments, 1) {
            Some(array) => context::encode_context_array(context, array)?,
            None => Vec::new(),
        };

        let mut rooms = Vec::new();
        if let Some(array) = args::array(&arguments, 2) {
            for (_, item) in array.iter() {
                let derefed = item.deref();
                let bytes = match &*derefed {
                    Value::String(s) => s.as_bytes(),
                    Value::InternedStr(s) => s.as_bytes(),
                    _ => {
                        return Err(NativeError::throw(
                            "TypeError",
                            "Phera\\WebSocket\\accept(): room names must be strings",
                        ));
                    }
                };
                let room = std::str::from_utf8(bytes).map_err(|_| {
                    NativeError::throw("ValueError", "Room name must be valid UTF-8")
                })?;
                if room.is_empty() {
                    return Err(NativeError::throw(
                        "ValueError",
                        "Room name must not be empty",
                    ));
                }
                rooms.push(room.to_owned());
            }
        }

        let subprotocol = if args::given(&arguments, 3) && !args::value(&arguments, 3).is_null() {
            let text = utf8_arg(&arguments, 3)?;
            if !is_valid_subprotocol(&text) {
                return Err(NativeError::throw(
                    "ValueError",
                    "WebSocket subprotocol must be a non-empty RFC 6455 token",
                ));
            }
            Some(text)
        } else {
            None
        };

        context.state().get_or_default::<PendingWebSocketAccept>().0 = Some(WebSocketAccept {
            handler: canonical_handler,
            context: context_bytes,
            rooms,
            subprotocol,
        });
        Ok(Value::Null)
    })
}

fn broadcast_fn(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let room = utf8_arg(arguments, 0)?;
    let data = args::bytes(arguments, 1);
    let binary = args::bool(arguments, 2).unwrap_or(false);
    validate_outbound_payload(data, binary)?;
    let sent = context
        .websocket_sink()
        .map_or(0, |sink| sink.broadcast(&room, data, binary, None));
    Ok(Value::Int(sent as i64))
}

fn send_fn(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let connection_id = utf8_arg(arguments, 0)?;
    let data = args::bytes(arguments, 1);
    let binary = args::bool(arguments, 2).unwrap_or(false);
    validate_outbound_payload(data, binary)?;
    let ok = context
        .websocket_sink()
        .is_some_and(|sink| sink.send(&connection_id, data, binary));
    Ok(Value::Bool(ok))
}

fn room_count_fn(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let room = utf8_arg(arguments, 0)?;
    let count = context
        .websocket_sink()
        .map_or(0, |sink| sink.room_count(&room));
    Ok(Value::Int(count as i64))
}

fn connections_fn(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let room = utf8_arg(arguments, 0)?;
    let mut array = PhpArray::new();
    if let Some(sink) = context.websocket_sink() {
        for id in sink.connections(&room) {
            let _ = array.push(Value::from(id));
        }
    }
    Ok(Value::from(array))
}

fn resolve_using_fn(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let resolver = arguments.first().cloned().unwrap_or(Value::Null);
    context.set_websocket_resolver(resolver)?;
    Ok(Value::Null)
}

fn is_event_fn(context: &mut dyn Context, _arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(context.is_websocket_event()))
}

fn is_valid_subprotocol(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

#[cfg(test)]
mod tests {
    use super::WEBSOCKET_PHP;

    #[test]
    fn bundled_websocket_php_compiles() {
        phera_compiler::compile(WEBSOCKET_PHP.source, WEBSOCKET_PHP.file)
            .expect("WebSocket.php must compile cleanly");
    }
}
