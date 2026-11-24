//! PHP-facing `Phera\WebSocket\*` API backed by [`phera_extension_api::websocket::WebSocketSink`].
//!
//! ```php
//! if (!Phera\WebSocket\isEvent()) {
//!     Phera\WebSocket\accept(ChatHandler::class, context: ['user' => 1], rooms: ['lobby']);
//! }
//! ```

pub(crate) mod context;

use std::rc::Rc;

use phera_extension_api::websocket::WebSocketAccept;
use phera_extension_api::{
    BundledSource, ClassEntry, ClassKind, ConstantValue, Context, FunctionEntry, LocalFuture,
    MethodEntry, NativeError, NativeResult, Object, Param, ParamType, PhpArray, PropertyEntry,
    PropertyType, Registry, Value, Visibility, args, fresh_state,
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
    register_connection_class(registry);
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

fn register_connection_class(registry: &mut Registry) {
    const SET_CONTEXT_PARAMS: &[Param] = &[Param::new("context", ParamType::Mixed)];
    const SEND_PARAMS: &[Param] = &[
        Param::new("data", ParamType::String),
        Param::new("binary", ParamType::Bool).with_default(ConstantValue::Bool(false)),
    ];
    const SEND_TO_PARAMS: &[Param] = &[
        Param::new("connectionId", ParamType::String),
        Param::new("data", ParamType::String),
        Param::new("binary", ParamType::Bool).with_default(ConstantValue::Bool(false)),
    ];
    const BROADCAST_PARAMS: &[Param] = &[
        Param::new("room", ParamType::String),
        Param::new("data", ParamType::String),
        Param::new("binary", ParamType::Bool).with_default(ConstantValue::Bool(false)),
        Param::new("includeSelf", ParamType::Bool).with_default(ConstantValue::Bool(false)),
    ];
    const ROOM_PARAM: &[Param] = &[Param::new("room", ParamType::String)];
    const CLOSE_PARAMS: &[Param] = &[
        Param::new("code", ParamType::Int).with_default(ConstantValue::Int(1000)),
        Param::new("reason", ParamType::String).with_default(ConstantValue::String("")),
    ];

    const PROPERTIES: &[PropertyEntry] = &[PropertyEntry::typed(
        "id",
        PropertyType::String,
        Visibility::Private,
    )];
    const METHODS: &[MethodEntry] = &[
        MethodEntry::public("__construct", &[], conn_private_construct)
            .with_visibility(Visibility::Private),
        MethodEntry::public("id", &[], conn_id).returning("string"),
        MethodEntry::public("context", &[], conn_context).returning("mixed"),
        MethodEntry::public("setContext", SET_CONTEXT_PARAMS, conn_set_context).returning("void"),
        MethodEntry::public("send", SEND_PARAMS, conn_send).returning("bool"),
        MethodEntry::public("sendTo", SEND_TO_PARAMS, conn_send_to).returning("bool"),
        MethodEntry::public("broadcast", BROADCAST_PARAMS, conn_broadcast).returning("int"),
        MethodEntry::public("join", ROOM_PARAM, conn_join).returning("bool"),
        MethodEntry::public("leave", ROOM_PARAM, conn_leave).returning("bool"),
        MethodEntry::public("rooms", &[], conn_rooms).returning("array"),
        MethodEntry::public("close", CLOSE_PARAMS, conn_close).returning("bool"),
    ];

    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        properties: PROPERTIES,
        methods: METHODS,
        ..ClassEntry::class(CONNECTION_CLASS)
    });
}

fn utf8_arg(arguments: &[Value], index: usize) -> Result<String, NativeError> {
    std::str::from_utf8(args::bytes(arguments, index))
        .map(str::to_owned)
        .map_err(|_| NativeError::throw("ValueError", "Argument must be valid UTF-8"))
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
    let sent = context
        .websocket_sink()
        .map_or(0, |sink| sink.broadcast(&room, data, binary, None));
    Ok(Value::Int(sent as i64))
}

fn send_fn(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let connection_id = utf8_arg(arguments, 0)?;
    let data = args::bytes(arguments, 1);
    let binary = args::bool(arguments, 2).unwrap_or(false);
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

fn conn_private_construct(
    _context: &mut dyn Context,
    _this: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    Ok(Value::Null)
}

fn connection_id(this: Option<&Rc<Object>>) -> Result<String, NativeError> {
    let object = this.ok_or_else(|| NativeError::error("Connection receiver missing"))?;
    let key = Object::private_key(CONNECTION_CLASS, "id");
    let value = object
        .get(&key)
        .ok_or_else(|| NativeError::error("Connection::$id is not initialized"))?;
    let Some(bytes) = value.as_str_bytes() else {
        return Err(NativeError::error("Connection::$id must be string"));
    };
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn conn_id(
    _context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    Ok(Value::from(connection_id(this)?))
}

fn conn_context(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    let id = connection_id(this)?;
    let bytes = context
        .websocket_sink()
        .and_then(|sink| sink.context(&id))
        .unwrap_or_default();
    Ok(context::decode_context(&bytes))
}

fn conn_set_context(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let id = connection_id(this)?;
    let value = arguments.first().unwrap_or(&Value::Null);
    let encoded = context::encode_context(context, value)?;
    if let Some(sink) = context.websocket_sink() {
        let _ = sink.set_context(&id, encoded);
    }
    Ok(Value::Null)
}

fn conn_send(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let id = connection_id(this)?;
    let data = args::bytes(arguments, 0);
    let binary = args::bool(arguments, 1).unwrap_or(false);
    let ok = context
        .websocket_sink()
        .is_some_and(|sink| sink.send(&id, data, binary));
    Ok(Value::Bool(ok))
}

fn conn_send_to(
    context: &mut dyn Context,
    _this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    send_fn(context, arguments)
}

fn conn_broadcast(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let id = connection_id(this)?;
    let room = utf8_arg(arguments, 0)?;
    let data = args::bytes(arguments, 1);
    let binary = args::bool(arguments, 2).unwrap_or(false);
    let include_self = args::bool(arguments, 3).unwrap_or(false);
    let exclude = (!include_self).then_some(id.as_str());
    let sent = context
        .websocket_sink()
        .map_or(0, |sink| sink.broadcast(&room, data, binary, exclude));
    Ok(Value::Int(sent as i64))
}

fn conn_join(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let id = connection_id(this)?;
    let room = utf8_arg(arguments, 0)?;
    if room.is_empty() {
        return Err(NativeError::throw(
            "ValueError",
            "Room name must not be empty",
        ));
    }
    let ok = context
        .websocket_sink()
        .is_some_and(|sink| sink.join(&id, &room));
    Ok(Value::Bool(ok))
}

fn conn_leave(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let id = connection_id(this)?;
    let room = utf8_arg(arguments, 0)?;
    let ok = context
        .websocket_sink()
        .is_some_and(|sink| sink.leave(&id, &room));
    Ok(Value::Bool(ok))
}

fn conn_rooms(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _arguments: &[Value],
) -> NativeResult {
    let id = connection_id(this)?;
    let mut array = PhpArray::new();
    if let Some(sink) = context.websocket_sink() {
        for room in sink.rooms(&id) {
            let _ = array.push(Value::from(room));
        }
    }
    Ok(Value::from(array))
}

fn conn_close(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let id = connection_id(this)?;
    let code = args::int(arguments, 0).unwrap_or(1000);
    if code != 1000 && !(3000..=4999).contains(&code) {
        return Err(NativeError::throw(
            "ValueError",
            "WebSocket close code must be 1000 or in 3000..4999",
        ));
    }
    let reason = if args::given(arguments, 1) {
        utf8_arg(arguments, 1)?
    } else {
        String::new()
    };
    if reason.len() > 123 {
        return Err(NativeError::throw(
            "ValueError",
            "WebSocket close reason must not exceed 123 bytes",
        ));
    }
    let ok = context
        .websocket_sink()
        .is_some_and(|sink| sink.close(&id, code as u16, &reason));
    Ok(Value::Bool(ok))
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
