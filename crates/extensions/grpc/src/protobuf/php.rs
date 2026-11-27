//! `Phera\Protobuf\Schema` and `Phera\Protobuf\Exception`.
//!
//! ```php
//! $schema = Schema::fromDescriptor(file_get_contents('set.pb'));
//! $schema->decode('echo.Req', $schema->encode('echo.Req', ['message' => 'hi']));
//! ```

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, ClassKind, ConstantValue, Context, MethodEntry, NativeError,
    NativeResult, Object, Param, ParamType, PhpArray, Registry, Value, args, fork_clone,
};

use super::Schema;

pub const SCHEMA_CLASS: &str = "Phera\\Protobuf\\Schema";
pub const EXCEPTION_CLASS: &str = "Phera\\Protobuf\\Exception";

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        parent: Some("RuntimeException"),
        ..ClassEntry::class(EXCEPTION_CLASS)
    });
    const DESCRIPTOR_PARAMS: &[Param] = &[Param::new("descriptorSet", ParamType::String)];
    const DEFINE_PARAMS: &[Param] = &[
        Param::new("messages", ParamType::Array),
        Param::new("services", ParamType::Array).with_default(ConstantValue::EmptyArray),
    ];
    const ENCODE_PARAMS: &[Param] = &[
        Param::new("messageType", ParamType::String),
        Param::new("data", ParamType::Array),
    ];
    const DECODE_PARAMS: &[Param] = &[
        Param::new("messageType", ParamType::String),
        Param::new("bytes", ParamType::String),
    ];
    const METHODS: &[MethodEntry] = &[
        MethodEntry::public("fromDescriptor", DESCRIPTOR_PARAMS, from_descriptor)
            .static_()
            .returning(SCHEMA_CLASS),
        MethodEntry::public("define", DEFINE_PARAMS, define)
            .static_()
            .returning(SCHEMA_CLASS),
        MethodEntry::public("encode", ENCODE_PARAMS, encode).returning("string"),
        MethodEntry::public("decode", DECODE_PARAMS, decode).returning("array"),
        MethodEntry::public("messages", &[], messages).returning("array"),
        MethodEntry::public("methods", &[], methods).returning("array"),
    ];
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        methods: METHODS,
        hooks: Some(&HOOKS),
        ..ClassEntry::class(SCHEMA_CLASS)
    });
}

/// A schema never changes after it is built, so a fork of a snapshot shares it.
const HOOKS: ClassHooks = ClassHooks {
    construction_error: Some(
        "Cannot directly construct Phera\\Protobuf\\Schema, use Schema::define() or Schema::fromDescriptor()",
    ),
    fork: Some(fork_clone::<Rc<Schema>>),
    forbid_serialization: true,
    ..ClassHooks::EMPTY
};

pub fn exception(message: impl Into<String>) -> NativeError {
    NativeError::throw(EXCEPTION_CLASS, message)
}

/// The schema inside a `Schema` object.
pub fn schema_of(object: &Object) -> Option<Rc<Schema>> {
    object
        .native
        .borrow()
        .as_ref()
        .and_then(|native| native.downcast_ref::<Rc<Schema>>())
        .cloned()
}

/// The schema of an optional `?Schema` argument.
pub fn schema_argument(arguments: &[Value], index: usize) -> Option<Rc<Schema>> {
    args::object(arguments, index).and_then(|object| schema_of(object))
}

fn this_schema(this: Option<&Rc<Object>>) -> Result<Rc<Schema>, NativeError> {
    this.and_then(|object| schema_of(object))
        .ok_or_else(|| NativeError::error("Phera\\Protobuf\\Schema is not initialized"))
}

pub fn message_id(schema: &Schema, name: &[u8]) -> Result<u32, NativeError> {
    let name = String::from_utf8_lossy(name);
    schema
        .message_id(&name)
        .ok_or_else(|| exception(format!("Unknown message type \"{name}\"")))
}

fn wrap(context: &mut dyn Context, schema: Schema) -> NativeResult {
    let object = context.create_object(SCHEMA_CLASS, PhpArray::new())?;
    if let Value::Object(object) = &object {
        *object.native.borrow_mut() = Some(Box::new(Rc::new(schema)));
    }
    Ok(object)
}

fn from_descriptor(
    context: &mut dyn Context,
    _: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let schema = super::descriptor::parse(args::bytes(arguments, 0)).map_err(exception)?;
    wrap(context, schema)
}

fn define(context: &mut dyn Context, _: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let empty = Rc::new(PhpArray::new());
    let messages = args::array(arguments, 0).unwrap_or(&empty);
    let services = args::array(arguments, 1).unwrap_or(&empty);
    let schema = super::define::parse(messages, services).map_err(exception)?;
    wrap(context, schema)
}

fn encode(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let schema = this_schema(this)?;
    let id = message_id(&schema, args::bytes(arguments, 0))?;
    let empty = Rc::new(PhpArray::new());
    let data = args::array(arguments, 1).unwrap_or(&empty);
    let mut out = Vec::with_capacity(64);
    super::encode(&schema, id, data, &mut out).map_err(|error| exception(error.to_string()))?;
    Ok(Value::from(out))
}

fn decode(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let schema = this_schema(this)?;
    let id = message_id(&schema, args::bytes(arguments, 0))?;
    let array = super::decode(&schema, id, args::bytes(arguments, 1))
        .map_err(|error| exception(error.to_string()))?;
    Ok(Value::from(array))
}

fn messages(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let schema = this_schema(this)?;
    let names: Vec<Value> = schema.message_names().map(Value::from).collect();
    let len = names.len();
    Ok(Value::from(PhpArray::from_values(names, len)))
}

/// `['/pkg.Service/Method' => ['input' => ..., 'output' => ..., ...]]`.
fn methods(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let schema = this_schema(this)?;
    let mut array = PhpArray::new();
    for method in schema.methods() {
        let mut entry = PhpArray::new();
        let name = |id| Value::from(&*schema.message(id).name);
        entry.insert(ArrayKey::str("input"), name(method.input));
        entry.insert(ArrayKey::str("output"), name(method.output));
        entry.insert(
            ArrayKey::str("clientStreaming"),
            Value::Bool(method.client_streaming),
        );
        entry.insert(
            ArrayKey::str("serverStreaming"),
            Value::Bool(method.server_streaming),
        );
        array.insert(ArrayKey::str(&*method.path), Value::from(entry));
    }
    Ok(Value::from(array))
}
