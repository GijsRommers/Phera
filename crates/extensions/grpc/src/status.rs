//! gRPC status codes: `Phera\Grpc\Status` and `Phera\Grpc\StatusException`.
//!
//! ```php
//! throw new StatusException(Status::NOT_FOUND, 'no user 42', ['retry-after' => '5']);
//! ```

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassKind, ConstantValue, Context, MethodEntry, NativeError,
    NativeResult, Object, Param, ParamType, PhpArray, PropertyEntry, PropertyType, Registry, Value,
    Visibility, args,
};

pub const STATUS_CLASS: &str = "Phera\\Grpc\\Status";
pub const EXCEPTION_CLASS: &str = "Phera\\Grpc\\StatusException";

pub const OK: u32 = 0;
pub const CANCELLED: u32 = 1;
pub const UNKNOWN: u32 = 2;
pub const DEADLINE_EXCEEDED: u32 = 4;
pub const NOT_FOUND: u32 = 5;
pub const PERMISSION_DENIED: u32 = 7;
pub const RESOURCE_EXHAUSTED: u32 = 8;
pub const UNIMPLEMENTED: u32 = 12;
pub const INTERNAL: u32 = 13;
pub const UNAVAILABLE: u32 = 14;
pub const UNAUTHENTICATED: u32 = 16;

/// The names of the codes, indexed by code.
pub const NAMES: [&str; 17] = [
    "OK",
    "CANCELLED",
    "UNKNOWN",
    "INVALID_ARGUMENT",
    "DEADLINE_EXCEEDED",
    "NOT_FOUND",
    "ALREADY_EXISTS",
    "PERMISSION_DENIED",
    "RESOURCE_EXHAUSTED",
    "FAILED_PRECONDITION",
    "ABORTED",
    "OUT_OF_RANGE",
    "UNIMPLEMENTED",
    "INTERNAL",
    "UNAVAILABLE",
    "DATA_LOSS",
    "UNAUTHENTICATED",
];

/// The result of a call that did not succeed, before it becomes trailers
/// or an exception.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub code: u32,
    pub message: String,
}

impl Status {
    pub fn new(code: u32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// The code a client reports for an HTTP status without `grpc-status`, as
/// the gRPC HTTP/2 mapping prescribes.
pub fn from_http(status: u16) -> u32 {
    match status {
        400 => INTERNAL,
        401 => UNAUTHENTICATED,
        403 => PERMISSION_DENIED,
        404 => UNIMPLEMENTED,
        429 | 502 | 503 | 504 => UNAVAILABLE,
        _ => UNKNOWN,
    }
}

/// A `StatusException` for the PHP caller, with trailers as its metadata.
pub fn exception(status: Status, metadata: PhpArray) -> NativeError {
    NativeError::throw(EXCEPTION_CLASS, status.message.clone())
        .with_code(i64::from(status.code))
        .with_property("grpcMessage", Value::from(status.message))
        .with_property("metadata", Value::from(metadata))
}

pub fn register(registry: &mut Registry) {
    const CONSTANTS: &[(&str, ConstantValue)] = &[
        ("OK", ConstantValue::Int(0)),
        ("CANCELLED", ConstantValue::Int(1)),
        ("UNKNOWN", ConstantValue::Int(2)),
        ("INVALID_ARGUMENT", ConstantValue::Int(3)),
        ("DEADLINE_EXCEEDED", ConstantValue::Int(4)),
        ("NOT_FOUND", ConstantValue::Int(5)),
        ("ALREADY_EXISTS", ConstantValue::Int(6)),
        ("PERMISSION_DENIED", ConstantValue::Int(7)),
        ("RESOURCE_EXHAUSTED", ConstantValue::Int(8)),
        ("FAILED_PRECONDITION", ConstantValue::Int(9)),
        ("ABORTED", ConstantValue::Int(10)),
        ("OUT_OF_RANGE", ConstantValue::Int(11)),
        ("UNIMPLEMENTED", ConstantValue::Int(12)),
        ("INTERNAL", ConstantValue::Int(13)),
        ("UNAVAILABLE", ConstantValue::Int(14)),
        ("DATA_LOSS", ConstantValue::Int(15)),
        ("UNAUTHENTICATED", ConstantValue::Int(16)),
    ];
    const NAME_PARAMS: &[Param] = &[Param::new("code", ParamType::Int)];
    const STATUS_METHODS: &[MethodEntry] = &[
        MethodEntry::public("__construct", &[], nothing).with_visibility(Visibility::Private),
        MethodEntry::public("name", NAME_PARAMS, name)
            .static_()
            .returning("string"),
    ];
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        constants: CONSTANTS,
        methods: STATUS_METHODS,
        ..ClassEntry::class(STATUS_CLASS)
    });

    const PROPERTIES: &[PropertyEntry] = &[
        PropertyEntry::typed("grpcMessage", PropertyType::String, Visibility::Public).readonly(),
        PropertyEntry::typed("metadata", PropertyType::Array, Visibility::Public).readonly(),
    ];
    const CONSTRUCT_PARAMS: &[Param] = &[
        Param::new("code", ParamType::Int),
        Param::new("grpcMessage", ParamType::String).with_default(ConstantValue::String("")),
        Param::new("metadata", ParamType::Array).with_default(ConstantValue::EmptyArray),
        Param::new("previous", ParamType::Class("Throwable"))
            .nullable()
            .with_default(ConstantValue::Null),
    ];
    const METHODS: &[MethodEntry] = &[
        MethodEntry::public("__construct", CONSTRUCT_PARAMS, construct),
        MethodEntry::public("getStatusCode", &[], status_code).returning("int"),
        MethodEntry::public("getGrpcMessage", &[], grpc_message).returning("string"),
        MethodEntry::public("getMetadata", &[], metadata).returning("array"),
    ];
    registry.class(ClassEntry {
        parent: Some("Exception"),
        properties: PROPERTIES,
        methods: METHODS,
        ..ClassEntry::class(EXCEPTION_CLASS)
    });
}

fn nothing(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::Null)
}

/// `Status::name(5)` is `"NOT_FOUND"`; an unknown code gives `"UNKNOWN"`.
fn name(_: &mut dyn Context, _: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let code = args::int(arguments, 0).unwrap_or(-1);
    let name = usize::try_from(code)
        .ok()
        .and_then(|code| NAMES.get(code))
        .unwrap_or(&"UNKNOWN");
    Ok(Value::from(*name))
}

/// The message of the exception is the gRPC message, its code the status.
fn construct(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let Some(object) = this else {
        return Ok(Value::Null);
    };
    let code = args::int(arguments, 0).unwrap_or(i64::from(UNKNOWN));
    let message = args::value(arguments, 1).clone();
    let message = if message.is_string() {
        message
    } else {
        Value::interned(b"")
    };
    let metadata = match args::value(arguments, 2) {
        Value::Array(array) => Value::Array(array.clone()),
        _ => Value::from(PhpArray::new()),
    };
    object.set(Object::protected_key("message"), message.clone());
    object.set(Object::protected_key("code"), Value::Int(code));
    if let Some(previous) = args::object(arguments, 3) {
        object.set(
            Object::private_key("Exception", "previous"),
            Value::Object(previous.clone()),
        );
    }
    object.set(ArrayKey::str("grpcMessage"), message);
    object.set(ArrayKey::str("metadata"), metadata);
    Ok(Value::Null)
}

fn status_code(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(this
        .and_then(|object| object.get(&Object::protected_key("code")))
        .filter(|code| matches!(code, Value::Int(_)))
        .unwrap_or(Value::Int(i64::from(UNKNOWN))))
}

fn grpc_message(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(this
        .and_then(|object| object.get(&ArrayKey::str("grpcMessage")))
        .unwrap_or_else(|| Value::interned(b"")))
}

fn metadata(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(this
        .and_then(|object| object.get(&ArrayKey::str("metadata")))
        .unwrap_or_else(|| Value::from(PhpArray::new())))
}

/// The status a server replies with when a handler threw `exception`.
pub fn from_exception(exception: &Object) -> (Status, PhpArray) {
    let text = |value: Option<Value>| {
        value
            .and_then(|value| value.as_str_bytes().map(|bytes| bytes.to_vec()))
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default()
    };
    if !exception.class.is_instance_of(EXCEPTION_CLASS) {
        let message = text(exception.get(&Object::protected_key("message")));
        return (Status::new(UNKNOWN, message), PhpArray::new());
    }
    let code = match exception.get(&Object::protected_key("code")) {
        Some(Value::Int(code)) if (0..=16).contains(&code) => code as u32,
        _ => UNKNOWN,
    };
    let message = text(
        exception
            .get(&ArrayKey::str("grpcMessage"))
            .or_else(|| exception.get(&Object::protected_key("message"))),
    );
    let metadata = match exception.get(&ArrayKey::str("metadata")) {
        Some(Value::Array(array)) => (*array).clone(),
        _ => PhpArray::new(),
    };
    (Status::new(code, message), metadata)
}
