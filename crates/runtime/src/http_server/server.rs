//! The `Phera\Http\Server` class: construction, route registration and
//! `listen()`.
//!
//! ```php
//! $server = new Phera\Http\Server(':8080');     // every interface, port 8080
//! $server->get('/', fn() => 'home')
//!        ->post('/items', $create)
//!        ->fallback(fn() => Phera\Http\Response::text('nothing here', 404));
//! echo $server->address(), "\n";                // bound already: 0.0.0.0:8080
//! $server->listen();
//! ```
//!
//! The constructor binds the socket, so a script that asks for port 0 can
//! read the port the system gave it with `address()` before it listens.

use std::net::{SocketAddr, TcpListener as StdListener};
use std::rc::Rc;
use std::sync::Arc;

use phera_extension_api::{
    ClassEntry, ClassKind, ConstantValue, Context, LocalFuture, MethodEntry, NativeError,
    NativeResult, Object, Param, ParamType, Registry, Value, args,
};
use tokio::sync::watch;

use super::SERVER_CLASS;
use super::dispatch::{self, RouteTable, WorkerSetup};
use super::router::{Method, MethodFilter, RouteError, Router};
use super::worker::{LaunchSlot, WorkerRoleSlot};

/// The native state of a `Phera\Http\Server` object.
struct ServerData {
    local_addr: SocketAddr,
    workers: usize,
    /// Bound by the constructor; taken by `listen()`.
    listener: Option<StdListener>,
    routes: Router,
    /// The handler of each route, at its [`RouteId`](super::router::RouteId).
    handlers: Vec<Value>,
    fallback: Option<Value>,
    worker_start: Vec<Value>,
    /// Shared with every worker: `stop()` on any of them stops them all.
    stop: Arc<watch::Sender<bool>>,
    /// Set by `listen()`, which owns the routes from then on.
    listening: bool,
    /// This VM is an extra worker: worker 0 starts the others.
    worker_index: Option<usize>,
}

pub(super) fn register(registry: &mut Registry) {
    const CONSTRUCT: &[Param] = &[
        Param::new("addr", ParamType::String).with_default(ConstantValue::String("127.0.0.1:8080")),
        Param::new("workers", ParamType::Int).with_default(ConstantValue::Int(0)),
    ];
    const ROUTE: &[Param] = &[
        Param::new("path", ParamType::String),
        Param::new("handler", ParamType::Callable),
    ];
    const ROUTE_WITH_METHOD: &[Param] = &[
        Param::new("method", ParamType::String),
        Param::new("path", ParamType::String),
        Param::new("handler", ParamType::Callable),
    ];
    const HANDLER: &[Param] = &[Param::new("handler", ParamType::Callable)];
    const CALLBACK: &[Param] = &[Param::new("callback", ParamType::Callable)];
    const METHODS: &[MethodEntry] = &[
        MethodEntry::public("__construct", CONSTRUCT, construct),
        MethodEntry::public("get", ROUTE, get).returning("static"),
        MethodEntry::public("post", ROUTE, post).returning("static"),
        MethodEntry::public("put", ROUTE, put).returning("static"),
        MethodEntry::public("patch", ROUTE, patch).returning("static"),
        MethodEntry::public("delete", ROUTE, delete).returning("static"),
        MethodEntry::public("head", ROUTE, head).returning("static"),
        MethodEntry::public("options", ROUTE, options).returning("static"),
        MethodEntry::public("any", ROUTE, any).returning("static"),
        MethodEntry::public("route", ROUTE_WITH_METHOD, route).returning("static"),
        MethodEntry::public("fallback", HANDLER, fallback).returning("static"),
        MethodEntry::public("onWorkerStart", CALLBACK, on_worker_start).returning("static"),
        MethodEntry::public("address", &[], address).returning("string"),
        MethodEntry::public("workers", &[], workers).returning("int"),
        MethodEntry::public("stop", &[], stop).returning("void"),
        MethodEntry::public_async("listen", &[], listen).returning("void"),
    ];
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        methods: METHODS,
        ..ClassEntry::class(SERVER_CLASS)
    });
}

fn this_object(this: Option<&Rc<Object>>) -> Result<&Rc<Object>, NativeError> {
    this.ok_or_else(|| NativeError::error("Phera\\Http\\Server method called without an object"))
}

/// Runs `change` on the server's native state.
fn with_server<R>(
    this: Option<&Rc<Object>>,
    change: impl FnOnce(&mut ServerData) -> Result<R, NativeError>,
) -> Result<R, NativeError> {
    let object = this_object(this)?;
    let mut native = object.native.borrow_mut();
    let data = native
        .as_mut()
        .and_then(|native| native.downcast_mut::<ServerData>())
        .ok_or_else(|| NativeError::error("Phera\\Http\\Server object is not initialized"))?;
    change(data)
}

fn construct(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let object = this_object(this)?;
    if object.native.borrow().is_some() {
        return Err(NativeError::throw(
            "LogicException",
            "Phera\\Http\\Server::__construct() cannot be called twice",
        ));
    }
    let text = String::from_utf8_lossy(args::bytes(arguments, 0)).into_owned();
    let addr = parse_address(&text).ok_or_else(|| {
        NativeError::value_error(format!(
            "Phera\\Http\\Server::__construct(): Argument #1 ($addr) must be \"host:port\", \"{text}\" given"
        ))
    })?;
    let workers = match args::int(arguments, 1).unwrap_or(0) {
        0 => std::thread::available_parallelism().map_or(1, |count| count.get()),
        count if count > 0 => count as usize,
        _ => {
            return Err(NativeError::value_error(
                "Phera\\Http\\Server::__construct(): Argument #2 ($workers) must be greater than or equal to 0",
            ));
        }
    };

    // A VM booted as an extra worker serves on the socket worker 0 bound.
    let role = context
        .state()
        .get_or_default::<WorkerRoleSlot>()
        .0
        .as_mut()
        .and_then(|role| {
            let listener = role.listener.take()?;
            Some((role.index, listener, role.stop.clone()))
        });
    let (worker_index, listener, stop) = match role {
        Some((index, listener, stop)) => (Some(index), listener, stop),
        None => {
            let listener =
                phera_server::bind_worker_listener(addr, workers > 1).map_err(|error| {
                    NativeError::throw(
                        "RuntimeException",
                        format!("Phera\\Http\\Server: cannot listen on {addr}: {error}"),
                    )
                })?;
            let (stop, _) = watch::channel(false);
            (None, listener, Arc::new(stop))
        }
    };
    let local_addr = listener.local_addr().map_err(|error| {
        NativeError::throw("RuntimeException", format!("Phera\\Http\\Server: {error}"))
    })?;
    *object.native.borrow_mut() = Some(Box::new(ServerData {
        local_addr,
        workers,
        listener: Some(listener),
        routes: Router::new(),
        handlers: Vec::new(),
        fallback: None,
        worker_start: Vec::new(),
        stop,
        listening: false,
        worker_index,
    }));
    Ok(Value::Null)
}

/// `"127.0.0.1:8080"`, `"[::1]:8080"`, `"localhost:8080"`, or `":8080"`
/// for every interface, as Go's `net/http` writes it.
fn parse_address(text: &str) -> Option<SocketAddr> {
    if let Ok(addr) = text.parse() {
        return Some(addr);
    }
    let (host, port) = text.rsplit_once(':')?;
    let port: u16 = port.parse().ok()?;
    match host {
        "" => Some(SocketAddr::from(([0, 0, 0, 0], port))),
        "localhost" => Some(SocketAddr::from(([127, 0, 0, 1], port))),
        _ => None,
    }
}

fn add_route(
    this: Option<&Rc<Object>>,
    method: MethodFilter,
    path: &[u8],
    handler: &Value,
) -> NativeResult {
    let path = std::str::from_utf8(path).map_err(|_| {
        NativeError::value_error("Phera\\Http\\Server: a route path must be valid UTF-8")
    })?;
    with_server(this, |server| {
        refuse_while_listening(server)?;
        let route = server
            .routes
            .add(method, path)
            .map_err(|error| match error {
                RouteError::InvalidPattern(message) => NativeError::value_error(message),
                RouteError::Duplicate => {
                    let method = match method {
                        MethodFilter::Only(method) => method.as_str(),
                        MethodFilter::Any => "ANY",
                    };
                    NativeError::throw(
                        "LogicException",
                        format!("Phera\\Http\\Server: route {method} {path} is already registered"),
                    )
                }
            })?;
        debug_assert_eq!(route as usize, server.handlers.len());
        server.handlers.push(handler.deref().clone());
        Ok(())
    })?;
    Ok(Value::Object(this_object(this)?.clone()))
}

fn refuse_while_listening(server: &ServerData) -> Result<(), NativeError> {
    if server.listening {
        return Err(NativeError::throw(
            "LogicException",
            "Phera\\Http\\Server: routes cannot change after listen()",
        ));
    }
    Ok(())
}

fn add_method_route(
    this: Option<&Rc<Object>>,
    method: Method,
    arguments: &[Value],
) -> NativeResult {
    add_route(
        this,
        MethodFilter::Only(method),
        args::bytes(arguments, 0),
        args::value(arguments, 1),
    )
}

fn get(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    add_method_route(this, Method::Get, arguments)
}

fn post(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    add_method_route(this, Method::Post, arguments)
}

fn put(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    add_method_route(this, Method::Put, arguments)
}

fn patch(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    add_method_route(this, Method::Patch, arguments)
}

fn delete(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    add_method_route(this, Method::Delete, arguments)
}

fn head(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    add_method_route(this, Method::Head, arguments)
}

fn options(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    add_method_route(this, Method::Options, arguments)
}

fn any(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    add_route(
        this,
        MethodFilter::Any,
        args::bytes(arguments, 0),
        args::value(arguments, 1),
    )
}

/// `route('PATCH', '/x', $handler)`; `'ANY'` (or `'*'`) is `any()`.
fn route(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let name = String::from_utf8_lossy(args::bytes(arguments, 0)).into_owned();
    let method = if name.eq_ignore_ascii_case("ANY") || name == "*" {
        MethodFilter::Any
    } else {
        let method = Method::from_registration(&name).ok_or_else(|| {
            NativeError::value_error(format!(
                "Phera\\Http\\Server::route(): Argument #1 ($method) must be an HTTP method such as \"GET\" or \"ANY\", \"{name}\" given"
            ))
        })?;
        MethodFilter::Only(method)
    };
    add_route(
        this,
        method,
        args::bytes(arguments, 1),
        args::value(arguments, 2),
    )
}

fn fallback(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    with_server(this, |server| {
        refuse_while_listening(server)?;
        server.fallback = Some(args::value(arguments, 0).deref().clone());
        Ok(())
    })?;
    Ok(Value::Object(this_object(this)?.clone()))
}

fn on_worker_start(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    with_server(this, |server| {
        refuse_while_listening(server)?;
        server
            .worker_start
            .push(args::value(arguments, 0).deref().clone());
        Ok(())
    })?;
    Ok(Value::Object(this_object(this)?.clone()))
}

fn address(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_server(this, |server| {
        Ok(Value::from(server.local_addr.to_string()))
    })
}

fn workers(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_server(this, |server| Ok(Value::Int(server.workers as i64)))
}

/// Stops every worker: they finish the requests they have, then `listen()`
/// returns. Before `listen()`, it makes `listen()` return at once.
fn stop(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_server(this, |server| {
        server.stop.send_replace(true);
        Ok(Value::Null)
    })
}

fn listen(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let setup = with_server(this.as_ref(), |server| {
            if server.listening {
                return Err(NativeError::throw(
                    "LogicException",
                    "Phera\\Http\\Server: listen() was already called",
                ));
            }
            server.listening = true;
            let listener = server.listener.take().ok_or_else(|| {
                NativeError::throw("LogicException", "Phera\\Http\\Server has no socket")
            })?;
            let routes = RouteTable {
                router: std::mem::take(&mut server.routes),
                handlers: std::mem::take(&mut server.handlers),
                fallback: server.fallback.take(),
            };
            Ok(WorkerSetup {
                index: server.worker_index.unwrap_or(0),
                listener,
                routes: Rc::new(routes),
                worker_start: std::mem::take(&mut server.worker_start),
                stop: server.stop.clone(),
                workers: server.workers,
            })
        })?;

        match context.state().get_or_default::<WorkerRoleSlot>().0.take() {
            Some(role) => {
                role.output.go_live();
                dispatch::serve(context, setup).await?;
                // The script of an extra worker ends with its `listen()`:
                // what follows the call runs once, in worker 0.
                Err(context.exit_script())
            }
            None => {
                let launch = context.state().get_or_default::<LaunchSlot>().0.clone();
                dispatch::serve_with_workers(context, setup, launch).await
            }
        }
    })
}
