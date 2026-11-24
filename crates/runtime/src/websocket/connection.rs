//! `Phera\WebSocket\Connection` class registration and methods.

use std::rc::Rc;

use phera_extension_api::{
    ClassEntry, ClassKind, ConstantValue, Context, MethodEntry, NativeError, NativeResult, Object,
    Param, ParamType, PhpArray, PropertyEntry, PropertyType, Registry, Value, Visibility, args,
};

use super::{
    CONNECTION_CLASS, context as ws_context, send_fn, utf8_arg, validate_outbound_payload,
};

/// Registers the final `Phera\WebSocket\Connection` class.
pub(super) fn register(registry: &mut Registry) {
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
    Ok(ws_context::decode_context(&bytes))
}

fn conn_set_context(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let id = connection_id(this)?;
    let value = arguments.first().unwrap_or(&Value::Null);
    let encoded = ws_context::encode_context(context, value)?;
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
    validate_outbound_payload(data, binary)?;
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
    validate_outbound_payload(data, binary)?;
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
