//! `Phera\Grpc\Server`: gRPC over HTTP/2, with PHP handlers called in the
//! thread that runs the script.
//!
//! ```php
//! $server = new Server('0.0.0.0:50051', $schema);
//! $server->unary('/echo.Echo/Say', fn (array $req, Context $ctx) => ['message' => $req['message']],
//!                'echo.Req', 'echo.Resp')
//!        ->serverStream('/echo.Echo/Count', function (array $req) {
//!            for ($i = 1; $i <= $req['to']; $i++) yield ['n' => $i];
//!        }, 'echo.CountReq', 'echo.Number')
//!        ->enableHealthService()
//!        ->listen();   // until $server->stop(), SIGINT or SIGTERM
//! ```
//!
//! # How a call travels
//!
//! ```text
//!  listen()                      one future, polled by the script's thread
//!  ├── driver                    accepts sockets; every connection and stream
//!  │   ├── connection task       is a task in the driver's own set, so nothing
//!  │   │   └── stream task       needs a LocalSet or crosses threads
//!  │   │       checks the call, reads its frames, answers health checks and
//!  │   │       unknown methods itself, queues the rest ───────┐
//!  └── worker                                                 │ in-memory queue
//!      runs the PHP handler for each queued call, in order <──┘
//!      and writes its response straight onto the HTTP/2 stream
//! ```
//!
//! The worker and the driver run side by side in `listen()`, so streams keep
//! moving (and flow control is answered) while PHP code runs. PHP handlers
//! run one at a time, as PHP code always does on one thread.

mod connection;
mod dispatch;
mod response;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, ClassKind, ConstantValue, Context, LocalFuture, MethodEntry,
    NativeError, NativeResult, Object, Param, ParamType, PhpArray, Registry, Value, args,
    fork_refused_handle,
};

use crate::framing::DEFAULT_MAX_MESSAGE;
use crate::protobuf::Schema;
use crate::protobuf::php::{message_id, schema_argument};

pub const SERVER_CLASS: &str = "Phera\\Grpc\\Server";

/// The health checking protocol's one unary method.
pub const HEALTH_CHECK: &str = "/grpc.health.v1.Health/Check";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteKind {
    Unary,
    ServerStream,
    ClientStream,
}

/// A method and the PHP callable that answers it.
#[derive(Clone)]
pub struct Route {
    pub path: Box<str>,
    pub kind: RouteKind,
    pub handler: Value,
    /// Message types; without them the handler sees and returns bytes.
    pub request: Option<u32>,
    pub response: Option<u32>,
}

#[derive(Clone)]
struct Options {
    tls: Option<(String, String)>,
    max_message: usize,
    compress: bool,
    handle_signals: bool,
    shutdown_grace: Duration,
}

pub struct ServerState {
    address: String,
    schema: Option<Rc<Schema>>,
    routes: RefCell<Vec<Route>>,
    health: Cell<bool>,
    /// Set by `stop()`; the server finishes the calls it has and returns.
    stop: Cell<bool>,
    listening: Cell<bool>,
    options: Options,
}

pub fn register(registry: &mut Registry) {
    const CONSTRUCT: &[Param] = &[
        Param::new("address", ParamType::String)
            .with_default(ConstantValue::String("127.0.0.1:50051")),
        Param::new("schema", ParamType::Class("Phera\\Protobuf\\Schema"))
            .nullable()
            .with_default(ConstantValue::Null),
        Param::new("options", ParamType::Array).with_default(ConstantValue::EmptyArray),
    ];
    const ROUTE: &[Param] = &[
        Param::new("method", ParamType::String),
        Param::new("handler", ParamType::Callable),
        Param::new("requestType", ParamType::String)
            .nullable()
            .with_default(ConstantValue::Null),
        Param::new("responseType", ParamType::String)
            .nullable()
            .with_default(ConstantValue::Null),
    ];
    const METHODS: &[MethodEntry] = &[
        MethodEntry::public("__construct", CONSTRUCT, construct),
        MethodEntry::public("unary", ROUTE, unary).returning("static"),
        MethodEntry::public("serverStream", ROUTE, server_stream).returning("static"),
        MethodEntry::public("clientStream", ROUTE, client_stream).returning("static"),
        MethodEntry::public("enableHealthService", &[], enable_health).returning("static"),
        MethodEntry::public("stop", &[], stop).returning("void"),
        MethodEntry::public_async("listen", &[], listen).returning("void"),
    ];
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        methods: METHODS,
        hooks: Some(&HOOKS),
        ..ClassEntry::class(SERVER_CLASS)
    });
}

const HOOKS: ClassHooks = ClassHooks {
    fork: Some(fork_refused_handle),
    forbid_serialization: true,
    trace_native: Some(trace_handlers),
    ..ClassHooks::EMPTY
};

/// Handlers are PHP values; the cycle collector may need to see them.
fn trace_handlers(state: &dyn std::any::Any, visit: &mut dyn FnMut(&Value)) -> bool {
    let Some(state) = state.downcast_ref::<Rc<ServerState>>() else {
        return false;
    };
    for route in state.routes.borrow().iter() {
        visit(&route.handler);
    }
    true
}

fn server_state(this: Option<&Rc<Object>>) -> Result<Rc<ServerState>, NativeError> {
    this.and_then(|object| {
        object
            .native
            .borrow()
            .as_ref()
            .and_then(|native| native.downcast_ref::<Rc<ServerState>>())
            .cloned()
    })
    .ok_or_else(|| NativeError::error("Phera\\Grpc\\Server is not initialized"))
}

fn runtime_exception(message: impl Into<String>) -> NativeError {
    NativeError::throw("RuntimeException", message)
}

fn construct(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let Some(object) = this else {
        return Ok(Value::Null);
    };
    let address = String::from_utf8_lossy(args::bytes(arguments, 0)).into_owned();
    let empty = Rc::new(PhpArray::new());
    let options = parse_options(args::array(arguments, 2).unwrap_or(&empty))?;
    let state = ServerState {
        address,
        schema: schema_argument(arguments, 1),
        routes: RefCell::new(Vec::new()),
        health: Cell::new(false),
        stop: Cell::new(false),
        listening: Cell::new(false),
        options,
    };
    *object.native.borrow_mut() = Some(Box::new(Rc::new(state)));
    Ok(Value::Null)
}

/// `['tls' => ['cert' => file, 'key' => file], 'maxMessageBytes' => n,
/// 'compression' => 'gzip', 'handleSignals' => bool, 'shutdownTimeoutMs' => n]`.
fn parse_options(options: &PhpArray) -> Result<Options, NativeError> {
    let mut parsed = Options {
        tls: None,
        max_message: DEFAULT_MAX_MESSAGE,
        compress: false,
        handle_signals: true,
        shutdown_grace: Duration::from_secs(10),
    };
    for (key, value) in options.iter() {
        let value = value.deref();
        let name = match &key {
            ArrayKey::Int(number) => number.to_string(),
            ArrayKey::Interned(name) => name.to_string_lossy(),
            ArrayKey::String(name) => name.to_string_lossy(),
        };
        let invalid = || NativeError::value_error(format!("Invalid server option \"{name}\""));
        match name.as_str() {
            "tls" => {
                let Value::Array(tls) = &*value else {
                    return Err(invalid());
                };
                let file = |key: &str| {
                    tls.get(&ArrayKey::str(key))
                        .and_then(|value| value.deref().as_str_bytes().map(<[u8]>::to_vec))
                        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                };
                let (Some(certificate), Some(key)) = (file("cert"), file("key")) else {
                    return Err(NativeError::value_error(
                        "Server option \"tls\" needs \"cert\" and \"key\" files",
                    ));
                };
                parsed.tls = Some((certificate, key));
            }
            "maxMessageBytes" => match &*value {
                Value::Int(bytes) if *bytes > 0 => parsed.max_message = *bytes as usize,
                _ => return Err(invalid()),
            },
            "compression" => match value.as_str_bytes() {
                Some(b"gzip") => parsed.compress = true,
                Some(b"identity") => parsed.compress = false,
                _ => return Err(invalid()),
            },
            "handleSignals" => match &*value {
                Value::Bool(flag) => parsed.handle_signals = *flag,
                _ => return Err(invalid()),
            },
            "shutdownTimeoutMs" => match &*value {
                Value::Int(millis) if *millis >= 0 => {
                    parsed.shutdown_grace = Duration::from_millis(*millis as u64)
                }
                _ => return Err(invalid()),
            },
            _ => {
                return Err(NativeError::value_error(format!(
                    "Unknown server option \"{name}\""
                )));
            }
        }
    }
    Ok(parsed)
}

fn unary(c: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    add_route(c, this, arguments, RouteKind::Unary)
}

fn server_stream(
    c: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    add_route(c, this, arguments, RouteKind::ServerStream)
}

fn client_stream(
    c: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    add_route(c, this, arguments, RouteKind::ClientStream)
}

fn add_route(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
    kind: RouteKind,
) -> NativeResult {
    let state = server_state(this)?;
    let path = method_path(args::bytes(arguments, 0))?;
    let (request, response) = route_types(state.schema.as_deref(), &path, arguments)?;
    let route = Route {
        path: path.into_boxed_str(),
        kind,
        handler: args::value(arguments, 1).clone(),
        request,
        response,
    };
    let mut routes = state.routes.borrow_mut();
    // Registering a method again replaces its handler.
    match routes
        .iter_mut()
        .find(|existing| existing.path == route.path)
    {
        Some(existing) => *existing = route,
        None => routes.push(route),
    }
    Ok(this.map_or(Value::Null, |object| Value::Object(object.clone())))
}

/// `/pkg.Service/Method`; the leading slash may be left out.
pub fn method_path(method: &[u8]) -> Result<String, NativeError> {
    let method = String::from_utf8_lossy(method);
    let path = if method.starts_with('/') {
        method.into_owned()
    } else {
        format!("/{method}")
    };
    let valid = path[1..].split_once('/').is_some_and(|(service, name)| {
        !service.is_empty() && !name.is_empty() && !name.contains('/')
    });
    if !valid {
        return Err(NativeError::value_error(format!(
            "Invalid gRPC method \"{path}\", expected \"/package.Service/Method\""
        )));
    }
    Ok(path)
}

/// The message types of a route: as given, or the method's from the schema.
pub fn route_types(
    schema: Option<&Schema>,
    path: &str,
    arguments: &[Value],
) -> Result<(Option<u32>, Option<u32>), NativeError> {
    let given = |index: usize| -> Result<Option<u32>, NativeError> {
        let Some(name) = arguments.get(index).and_then(Value::as_str_bytes) else {
            return Ok(None);
        };
        let schema = schema.ok_or_else(|| {
            NativeError::throw(
                "LogicException",
                "Message types need a Phera\\Protobuf\\Schema; pass one to the constructor",
            )
        })?;
        message_id(schema, name).map(Some)
    };
    let request = given(2)?;
    let response = given(3)?;
    let declared = schema.and_then(|schema| schema.method(path));
    Ok((
        request.or(declared.map(|method| method.input)),
        response.or(declared.map(|method| method.output)),
    ))
}

fn enable_health(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    server_state(this)?.health.set(true);
    Ok(this.map_or(Value::Null, |object| Value::Object(object.clone())))
}

fn stop(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    server_state(this)?.stop.set(true);
    Ok(Value::Null)
}

fn listen<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let state = server_state(this.as_ref())?;
        if state.listening.replace(true) {
            return Err(NativeError::throw(
                "LogicException",
                "Phera\\Grpc\\Server::listen() is already running",
            ));
        }
        state.stop.set(false);
        let result = serve(context, &state).await;
        state.listening.set(false);
        result.map(|()| Value::Null)
    })
}

async fn serve(context: &mut dyn Context, state: &ServerState) -> Result<(), NativeError> {
    use futures_util::future::{Either, select};

    let tls = match &state.options.tls {
        Some((certificate, key)) => Some(tokio_rustls::TlsAcceptor::from(
            crate::tls::server_config(certificate, key).map_err(runtime_exception)?,
        )),
        None => None,
    };
    let listener = tokio::net::TcpListener::bind(&state.address)
        .await
        .map_err(|error| {
            runtime_exception(format!("Cannot listen on {}: {error}", state.address))
        })?;
    let mut routes = state.routes.borrow().clone();
    routes.sort_by(|a, b| a.path.cmp(&b.path));
    let table = Rc::new(connection::RouteTable::new(
        &routes,
        state.health.get(),
        state.options.max_message,
        state.options.compress,
    ));
    let (calls, queue) = tokio::sync::mpsc::unbounded_channel();
    let (shutdown, _) = tokio::sync::watch::channel(false);
    let shutdown = Rc::new(shutdown);

    let driver = std::pin::pin!(connection::drive(connection::Driver {
        listener,
        tls,
        table,
        calls,
        shutdown: shutdown.clone(),
        grace: state.options.shutdown_grace,
        handle_signals: state.options.handle_signals,
    }));
    let worker = std::pin::pin!(dispatch::work(
        context,
        &routes,
        state.schema.as_deref(),
        queue,
        state,
        &shutdown,
    ));
    match select(driver, worker).await {
        // Every connection is closed; the worker answers what was queued.
        Either::Left(((), worker)) => worker.await,
        // `exit()` in a handler: the connections close with the server.
        Either::Right((Err(error), _)) => Err(error),
        Either::Right((Ok(()), driver)) => {
            driver.await;
            Ok(())
        }
    }
}
