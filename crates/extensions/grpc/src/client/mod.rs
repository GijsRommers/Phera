//! `Phera\Grpc\Client`: calls over a pooled HTTP/2 channel.
//!
//! ```php
//! $client = new Client('https://api.example.com', $schema, ['timeoutMs' => 2000]);
//! $user = $client->unary('/users.Users/Get', ['id' => 42], 'users.GetRequest', 'users.User',
//!                        metadata: ['authorization' => "Bearer $token"]);
//! foreach ($client->serverStream('/feed.Feed/Watch', ['topic' => 'news']) as $event) { ... }
//! $client->lastTrailers();   // trailers of the last call
//! ```
//!
//! A call waits without blocking the thread: in `phera serve`, other
//! requests on the worker run meanwhile. A status other than OK, a broken
//! connection or an expired timeout throws `Phera\Grpc\StatusException`.

mod channel;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, TE, USER_AGENT};
use http::{Method, Request, Uri};
use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, ClassKind, ConstantValue, Context, LocalFuture, MethodEntry,
    NativeError, NativeResult, Object, Param, ParamType, PhpArray, Registry, Value, args,
};

use crate::framing::{self, DEFAULT_MAX_MESSAGE, Encoding};
use crate::headers;
use crate::protobuf::php::schema_argument;
use crate::protobuf::{self, Schema};
use crate::server::{method_path, route_types};
use crate::status::{self, DEADLINE_EXCEEDED, INTERNAL, OK, Status};
use crate::transport;

pub const CLIENT_CLASS: &str = "Phera\\Grpc\\Client";

const GRPC_STATUS: HeaderName = HeaderName::from_static("grpc-status");
const GRPC_MESSAGE: HeaderName = HeaderName::from_static("grpc-message");
const GRPC_ENCODING: HeaderName = HeaderName::from_static("grpc-encoding");
const GRPC_ACCEPT_ENCODING: HeaderName = HeaderName::from_static("grpc-accept-encoding");
const GRPC_TIMEOUT: HeaderName = HeaderName::from_static("grpc-timeout");

/// What a `Client` was constructed with; never changes, so forks share it.
struct Config {
    https: bool,
    /// `host:port`, for connecting and for `:authority`.
    address: String,
    authority: String,
    host: String,
    pool_key: String,
    schema: Option<Rc<Schema>>,
    ca_file: Option<String>,
    compress: bool,
    metadata: HeaderMap,
    timeout: Option<Duration>,
    connect_timeout: Duration,
    max_message: usize,
}

struct ClientState {
    config: Rc<Config>,
    /// Built on first use: loading the system's roots takes a moment.
    tls: RefCell<Option<Arc<rustls::ClientConfig>>>,
    last_headers: RefCell<PhpArray>,
    last_trailers: RefCell<PhpArray>,
}

pub fn register(registry: &mut Registry) {
    const CONSTRUCT: &[Param] = &[
        Param::new("endpoint", ParamType::String),
        Param::new("schema", ParamType::Class("Phera\\Protobuf\\Schema"))
            .nullable()
            .with_default(ConstantValue::Null),
        Param::new("options", ParamType::Array).with_default(ConstantValue::EmptyArray),
    ];
    const CALL: &[Param] = &[
        Param::new("method", ParamType::String),
        Param::new("request", ParamType::ArrayOrString),
        Param::new("requestType", ParamType::String)
            .nullable()
            .with_default(ConstantValue::Null),
        Param::new("responseType", ParamType::String)
            .nullable()
            .with_default(ConstantValue::Null),
        Param::new("metadata", ParamType::Array).with_default(ConstantValue::EmptyArray),
        Param::new("timeoutMs", ParamType::Int)
            .nullable()
            .with_default(ConstantValue::Null),
    ];
    const CLIENT_STREAM: &[Param] = &[
        Param::new("method", ParamType::String),
        Param::new("requests", ParamType::Array),
        Param::new("requestType", ParamType::String)
            .nullable()
            .with_default(ConstantValue::Null),
        Param::new("responseType", ParamType::String)
            .nullable()
            .with_default(ConstantValue::Null),
        Param::new("metadata", ParamType::Array).with_default(ConstantValue::EmptyArray),
        Param::new("timeoutMs", ParamType::Int)
            .nullable()
            .with_default(ConstantValue::Null),
    ];
    const METHODS: &[MethodEntry] = &[
        MethodEntry::public("__construct", CONSTRUCT, construct),
        MethodEntry::public_async("unary", CALL, unary).returning("array|string"),
        MethodEntry::public_async("serverStream", CALL, server_stream).returning("array"),
        MethodEntry::public_async("clientStream", CLIENT_STREAM, client_stream)
            .returning("array|string"),
        MethodEntry::public("lastHeaders", &[], last_headers).returning("array"),
        MethodEntry::public("lastTrailers", &[], last_trailers).returning("array"),
        MethodEntry::public("close", &[], close).returning("void"),
    ];
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        methods: METHODS,
        hooks: Some(&HOOKS),
        ..ClassEntry::class(CLIENT_CLASS)
    });
}

/// A client made before `Phera\snapshot()` works in every request: the
/// fork keeps the configuration, and channels live in the thread's pool.
const HOOKS: ClassHooks = ClassHooks {
    fork: Some(fork_client),
    forbid_serialization: true,
    ..ClassHooks::EMPTY
};

fn fork_client(
    _: &Object,
    state: &dyn std::any::Any,
    _: &mut phera_extension_api::value::Fork<'_>,
) -> Result<Box<dyn std::any::Any>, String> {
    let state = state
        .downcast_ref::<Rc<ClientState>>()
        .ok_or("its built-in state is not initialized")?;
    Ok(Box::new(Rc::new(ClientState {
        config: state.config.clone(),
        tls: RefCell::new(state.tls.borrow().clone()),
        last_headers: RefCell::new(PhpArray::new()),
        last_trailers: RefCell::new(PhpArray::new()),
    })))
}

fn client_state(this: Option<&Rc<Object>>) -> Result<Rc<ClientState>, NativeError> {
    this.and_then(|object| {
        object
            .native
            .borrow()
            .as_ref()
            .and_then(|native| native.downcast_ref::<Rc<ClientState>>())
            .cloned()
    })
    .ok_or_else(|| NativeError::error("Phera\\Grpc\\Client is not initialized"))
}

fn construct(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let Some(object) = this else {
        return Ok(Value::Null);
    };
    let empty = Rc::new(PhpArray::new());
    let options = args::array(arguments, 2).unwrap_or(&empty);
    let config = configure(
        &String::from_utf8_lossy(args::bytes(arguments, 0)),
        schema_argument(arguments, 1),
        options,
    )?;
    let state = ClientState {
        config: Rc::new(config),
        tls: RefCell::new(None),
        last_headers: RefCell::new(PhpArray::new()),
        last_trailers: RefCell::new(PhpArray::new()),
    };
    *object.native.borrow_mut() = Some(Box::new(Rc::new(state)));
    Ok(Value::Null)
}

/// `host:port`, `http://host:port` or `https://host[:port]`, and options
/// `tls`, `caFile`, `authority`, `compression`, `metadata`, `timeoutMs`,
/// `connectTimeoutMs`, `maxMessageBytes`.
fn configure(
    endpoint: &str,
    schema: Option<Rc<Schema>>,
    options: &PhpArray,
) -> Result<Config, NativeError> {
    let invalid_endpoint = || {
        NativeError::value_error(format!(
            "Invalid gRPC endpoint \"{endpoint}\", expected host:port or http(s)://host:port"
        ))
    };
    let (mut https, rest) = match endpoint.split_once("://") {
        Some(("https", rest)) => (true, rest),
        Some(("http", rest)) => (false, rest),
        Some(_) => return Err(invalid_endpoint()),
        None => (false, endpoint),
    };
    let authority = rest.trim_end_matches('/');
    if authority.is_empty() || authority.contains('/') {
        return Err(invalid_endpoint());
    }
    let mut config = Config {
        https,
        address: String::new(),
        authority: authority.to_owned(),
        host: String::new(),
        pool_key: String::new(),
        schema,
        ca_file: None,
        compress: false,
        metadata: HeaderMap::new(),
        timeout: None,
        connect_timeout: Duration::from_secs(10),
        max_message: DEFAULT_MAX_MESSAGE,
    };
    for (key, value) in options.iter() {
        let value = value.deref();
        let name = match &key {
            ArrayKey::Int(number) => number.to_string(),
            ArrayKey::Interned(name) => name.to_string_lossy(),
            ArrayKey::String(name) => name.to_string_lossy(),
        };
        let invalid = || NativeError::value_error(format!("Invalid client option \"{name}\""));
        let text = || {
            value
                .as_str_bytes()
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
                .ok_or_else(invalid)
        };
        let millis = || match &*value {
            Value::Int(millis) if *millis >= 0 => Ok(Duration::from_millis(*millis as u64)),
            _ => Err(invalid()),
        };
        match name.as_str() {
            "tls" => match &*value {
                Value::Bool(flag) => https = *flag,
                _ => return Err(invalid()),
            },
            "caFile" => config.ca_file = Some(text()?),
            "authority" => config.authority = text()?,
            "compression" => match value.as_str_bytes() {
                Some(b"gzip") => config.compress = true,
                Some(b"identity") => config.compress = false,
                _ => return Err(invalid()),
            },
            "metadata" => {
                let Value::Array(metadata) = &*value else {
                    return Err(invalid());
                };
                headers::append(&mut config.metadata, metadata)
                    .map_err(NativeError::value_error)?;
            }
            "timeoutMs" => config.timeout = Some(millis()?),
            "connectTimeoutMs" => config.connect_timeout = millis()?,
            "maxMessageBytes" => match &*value {
                Value::Int(bytes) if *bytes > 0 => config.max_message = *bytes as usize,
                _ => return Err(invalid()),
            },
            _ => {
                return Err(NativeError::value_error(format!(
                    "Unknown client option \"{name}\""
                )));
            }
        }
    }
    config.https = https;
    // The port defaults to the scheme's, as in a URL.
    let uri: Uri = format!("http://{authority}")
        .parse()
        .map_err(|_| invalid_endpoint())?;
    let host = uri.host().ok_or_else(invalid_endpoint)?;
    let port = uri.port_u16().unwrap_or(if https { 443 } else { 80 });
    config.host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_owned();
    config.address = if host.starts_with('[') {
        format!("{host}:{port}")
    } else {
        format!("{}:{port}", config.host)
    };
    config.pool_key = format!(
        "{}://{}#{}",
        if https { "https" } else { "http" },
        config.address,
        config.ca_file.as_deref().unwrap_or("")
    );
    Ok(config)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Unary,
    ServerStream,
    ClientStream,
}

fn unary<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(invoke(context, this, arguments, Shape::Unary))
}

fn server_stream<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(invoke(context, this, arguments, Shape::ServerStream))
}

fn client_stream<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(invoke(context, this, arguments, Shape::ClientStream))
}

async fn invoke(
    _context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    shape: Shape,
) -> NativeResult {
    let state = client_state(this.as_ref())?;
    let config = state.config.clone();
    let path = method_path(args::bytes(&arguments, 0))?;
    let schema = config.schema.as_deref();
    let (request_type, response_type) = route_types(schema, &path, &arguments)?;
    let encoding = if config.compress {
        Encoding::Gzip
    } else {
        Encoding::Identity
    };
    let frames = match shape {
        Shape::ClientStream => {
            let empty = Rc::new(PhpArray::new());
            let requests = args::array(&arguments, 1).unwrap_or(&empty);
            requests
                .values()
                .map(|request| request_frame(schema, request_type, &request.deref(), encoding))
                .collect::<Result<Vec<Bytes>, NativeError>>()?
        }
        _ => vec![request_frame(
            schema,
            request_type,
            args::value(&arguments, 1),
            encoding,
        )?],
    };
    let mut metadata = config.metadata.clone();
    if let Some(call_metadata) = args::array(&arguments, 4) {
        headers::append(&mut metadata, call_metadata).map_err(NativeError::value_error)?;
    }
    let timeout = match args::value(&arguments, 5) {
        Value::Int(millis) if *millis >= 0 => Some(Duration::from_millis(*millis as u64)),
        Value::Int(_) => return Err(NativeError::value_error("timeoutMs must not be negative")),
        _ => config.timeout,
    };
    let tls = tls_config(&state).await?;

    let result = call(&config, tls, &path, frames, metadata, timeout).await;
    let (headers, trailers) = match &result {
        Ok(reply) => (&reply.headers, &reply.trailers),
        Err(failure) => (&failure.headers, &failure.trailers),
    };
    *state.last_headers.borrow_mut() = headers::to_array(headers);
    *state.last_trailers.borrow_mut() = headers::to_array(trailers);
    let reply = match result {
        Ok(reply) => reply,
        Err(failure) => {
            let metadata = headers::to_array(&failure.trailers);
            return Err(status::exception(failure.status, metadata));
        }
    };

    let decode = |bytes: &Bytes| -> NativeResult {
        match (response_type, schema) {
            (Some(id), Some(schema)) => protobuf::decode(schema, id, bytes)
                .map(Value::from)
                .map_err(|error| {
                    status::exception(
                        Status::new(INTERNAL, format!("invalid response message: {error}")),
                        PhpArray::new(),
                    )
                }),
            _ => Ok(Value::from(&bytes[..])),
        }
    };
    if shape == Shape::ServerStream {
        let values = reply
            .messages
            .iter()
            .map(decode)
            .collect::<Result<Vec<Value>, NativeError>>()?;
        let len = values.len();
        return Ok(Value::from(PhpArray::from_values(values, len)));
    }
    match reply.messages.as_slice() {
        [message] => decode(message),
        messages => Err(status::exception(
            Status::new(
                INTERNAL,
                format!("expected one response message, got {}", messages.len()),
            ),
            PhpArray::new(),
        )),
    }
}

async fn tls_config(state: &ClientState) -> Result<Option<Arc<rustls::ClientConfig>>, NativeError> {
    if !state.config.https {
        return Ok(None);
    }
    if let Some(config) = state.tls.borrow().clone() {
        return Ok(Some(config));
    }
    let config = crate::tls::client_config(state.config.ca_file.as_deref())
        .await
        .map_err(|error| NativeError::throw("RuntimeException", error))?;
    *state.tls.borrow_mut() = Some(config.clone());
    Ok(Some(config))
}

fn request_frame(
    schema: Option<&Schema>,
    request_type: Option<u32>,
    request: &Value,
    encoding: Encoding,
) -> Result<Bytes, NativeError> {
    if let Some(bytes) = request.as_str_bytes() {
        return Ok(framing::frame(bytes, encoding));
    }
    let Value::Array(data) = request else {
        return Err(NativeError::type_error(format!(
            "A gRPC request must be a string or an array, {} given",
            request.type_name()
        )));
    };
    let (Some(id), Some(schema)) = (request_type, schema) else {
        return Err(NativeError::throw(
            "LogicException",
            "An array request needs a request type and a Phera\\Protobuf\\Schema",
        ));
    };
    let mut frame = framing::begin();
    protobuf::encode(schema, id, data, &mut frame)
        .map_err(|error| protobuf::php::exception(error.to_string()))?;
    Ok(framing::finish(frame, encoding))
}

fn last_headers(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::from(
        client_state(this)?.last_headers.borrow().clone(),
    ))
}

fn last_trailers(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::from(
        client_state(this)?.last_trailers.borrow().clone(),
    ))
}

/// Closes this endpoint's pooled channel; the next call opens a new one.
fn close(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    channel::forget(&client_state(this)?.config.pool_key);
    Ok(Value::Null)
}

struct Reply {
    headers: HeaderMap,
    messages: Vec<Bytes>,
    trailers: HeaderMap,
}

struct Failure {
    status: Status,
    headers: HeaderMap,
    trailers: HeaderMap,
}

/// Boxed: two header maps make it large, and it is the rare outcome.
impl From<Status> for Box<Failure> {
    fn from(status: Status) -> Self {
        Box::new(Failure {
            status,
            headers: HeaderMap::new(),
            trailers: HeaderMap::new(),
        })
    }
}

/// One call, under the timeout as a whole: connecting, sending, receiving.
async fn call(
    config: &Config,
    tls: Option<Arc<rustls::ClientConfig>>,
    path: &str,
    frames: Vec<Bytes>,
    metadata: HeaderMap,
    timeout: Option<Duration>,
) -> Result<Reply, Box<Failure>> {
    let exchange = exchange(config, tls, path, frames, metadata, timeout);
    match timeout {
        Some(timeout) => tokio::time::timeout(timeout, exchange)
            .await
            .unwrap_or_else(|_| Err(Status::new(DEADLINE_EXCEEDED, "Deadline Exceeded").into())),
        None => exchange.await,
    }
}

async fn exchange(
    config: &Config,
    tls: Option<Arc<rustls::ClientConfig>>,
    path: &str,
    frames: Vec<Bytes>,
    metadata: HeaderMap,
    timeout: Option<Duration>,
) -> Result<Reply, Box<Failure>> {
    let endpoint = channel::Endpoint {
        key: &config.pool_key,
        address: &config.address,
        host: &config.host,
        tls,
        connect_timeout: config.connect_timeout,
    };
    let mut sender = channel::ready(&endpoint).await?;
    let scheme = if config.https { "https" } else { "http" };
    let uri: Uri = format!("{scheme}://{}{path}", config.authority)
        .parse()
        .map_err(|_| Status::new(INTERNAL, format!("invalid method path {path}")))?;
    let mut request = Request::new(());
    *request.method_mut() = Method::POST;
    *request.uri_mut() = uri;
    let headers = request.headers_mut();
    *headers = metadata;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/grpc"));
    headers.insert(TE, HeaderValue::from_static("trailers"));
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(concat!("phera-grpc/", env!("CARGO_PKG_VERSION"))),
    );
    headers.insert(GRPC_ACCEPT_ENCODING, HeaderValue::from_static("gzip"));
    if config.compress {
        headers.insert(GRPC_ENCODING, HeaderValue::from_static("gzip"));
    }
    if let Some(timeout) = timeout
        && let Ok(value) = HeaderValue::from_str(&headers::format_timeout(timeout))
    {
        headers.insert(GRPC_TIMEOUT, value);
    }

    let (response, mut outgoing) = sender.send_request(request, false).map_err(|error| {
        Status::new(
            status::UNAVAILABLE,
            format!("cannot start the call: {error}"),
        )
    })?;
    let count = frames.len();
    if count == 0 {
        let _ = outgoing.send_data(Bytes::new(), true);
    }
    for (index, frame) in frames.into_iter().enumerate() {
        // A failed send shows up as the response's error below.
        if transport::send(&mut outgoing, frame, index + 1 == count)
            .await
            .is_err()
        {
            break;
        }
    }
    let response = response.await.map_err(|error| {
        Box::<Failure>::from(match error.reason() {
            Some(_) => transport::stream_error(error),
            None => Status::new(
                status::UNAVAILABLE,
                format!("the connection failed: {error}"),
            ),
        })
    })?;
    let (parts, mut body) = response.into_parts();
    // Trailers-only: the status came with the headers.
    if parts.headers.contains_key(GRPC_STATUS) {
        // Those headers are the call's trailers.
        let status = status_of(&parts.headers);
        return finish(status, HeaderMap::new(), parts.headers, Vec::new());
    }
    if parts.status != http::StatusCode::OK {
        let code = parts.status.as_u16();
        let status = Status::new(status::from_http(code), format!("HTTP status {code}"));
        return Err(Box::new(Failure {
            status,
            headers: parts.headers,
            trailers: HeaderMap::new(),
        }));
    }
    let encoding = parts.headers.get(GRPC_ENCODING).map(HeaderValue::as_bytes);
    let Some(encoding) = Encoding::parse(encoding) else {
        let status = framing::unsupported_encoding(encoding.unwrap_or_default());
        return Err(Box::new(Failure {
            status: Status::new(INTERNAL, status.message),
            headers: parts.headers,
            trailers: HeaderMap::new(),
        }));
    };
    let frames = match transport::read_frames(&mut body, config.max_message).await {
        Ok(frames) => frames,
        Err(status) => {
            return Err(Box::new(Failure {
                status,
                headers: parts.headers,
                trailers: HeaderMap::new(),
            }));
        }
    };
    let trailers = match body.trailers().await {
        Ok(trailers) => trailers.unwrap_or_default(),
        Err(error) => {
            return Err(Box::new(Failure {
                status: transport::stream_error(error),
                headers: parts.headers,
                trailers: HeaderMap::new(),
            }));
        }
    };
    let status = if trailers.contains_key(GRPC_STATUS) {
        status_of(&trailers)
    } else {
        Status::new(INTERNAL, "the server sent no grpc-status")
    };
    let mut messages = Vec::with_capacity(frames.len());
    for frame in frames {
        match frame.into_message(encoding, config.max_message) {
            Ok(message) => messages.push(message),
            Err(status) => {
                return Err(Box::new(Failure {
                    status,
                    headers: parts.headers,
                    trailers,
                }));
            }
        }
    }
    finish(status, parts.headers, trailers, messages)
}

fn finish(
    status: Status,
    headers: HeaderMap,
    trailers: HeaderMap,
    messages: Vec<Bytes>,
) -> Result<Reply, Box<Failure>> {
    if status.code == OK {
        Ok(Reply {
            headers,
            messages,
            trailers,
        })
    } else {
        Err(Box::new(Failure {
            status,
            headers,
            trailers,
        }))
    }
}

fn status_of(headers: &HeaderMap) -> Status {
    let code = headers
        .get(GRPC_STATUS)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|code| *code <= 16)
        .unwrap_or(status::UNKNOWN);
    let message = headers
        .get(GRPC_MESSAGE)
        .map(|value| headers::decode_message(value.as_bytes()))
        .unwrap_or_default();
    Status::new(code, message)
}
