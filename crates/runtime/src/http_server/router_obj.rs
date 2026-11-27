//! The `Phera\Http\Router` class and `Phera\Http\serve()` convenience handoff.

use std::any::Any;
use std::net::SocketAddr;
use std::rc::Rc;

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, ConstantValue, Context, FunctionEntry, MethodEntry,
    NativeError, NativeResult, Object, Param, ParamType, Registry, Value, args,
};

use super::router::{Method, MethodFilter, RouteError, Router};
use super::{ROUTER_CLASS, SERVE_FUNCTION};

/// Native state inside a `Phera\Http\Router` object.
pub(crate) struct RouterData {
    pub(crate) router: Rc<Router>,
    /// Route handler callables indexed by `RouteId`.
    pub(crate) handlers: Vec<Value>,
    pub(crate) fallback: Option<Value>,
    pub(crate) on_start: Vec<Value>,
    /// Set once the worker freezes the router; prevents mutating routes mid-flight.
    pub(crate) frozen: bool,
}

/// Armed on a VM during `WebWorker::boot_router` so a script calling
/// `Phera\Http\serve($router, ...)` hands its router straight to `phera serve`
/// instead of trying to start a nested server.
#[derive(Default)]
pub(crate) struct BootRouterSlot {
    pub(crate) active: bool,
    pub(crate) captured: Option<Rc<Object>>,
}

/// Recorded on a CLI VM when `phera app.php` calls `Phera\Http\serve($router, ...)`.
/// The CLI driver reads this after `run_file_blocking` and launches the unified
/// `phera serve` engine on the same script.
#[derive(Clone, Debug)]
pub struct PendingServe {
    pub addr: SocketAddr,
    /// `None` means one worker per CPU core (`workers: 0`).
    pub workers: Option<usize>,
}

#[derive(Default)]
pub(crate) struct PendingServeSlot(pub(crate) Option<PendingServe>);

/// Holds the active `Phera\Http\Router` object inside a route-snapshot VM so
/// every `Snapshot::fork()` copies the router's handlers and captured closures
/// inside the same `Fork` graph as the VM's globals and objects.
#[derive(Default)]
pub(crate) struct ActiveRouterSlot(pub(crate) Option<Rc<Object>>);

pub(super) fn fork_active_router_slot(
    state: &dyn Any,
    fork: &mut phera_value::Fork<'_>,
) -> Option<Box<dyn Any>> {
    let slot = state.downcast_ref::<ActiveRouterSlot>()?;
    let router = slot.0.as_ref().map(|obj| fork.object(obj));
    Some(Box::new(ActiveRouterSlot(router)))
}

fn fork_router(
    _object: &Object,
    state: &dyn Any,
    fork: &mut phera_value::Fork<'_>,
) -> Result<Box<dyn Any>, String> {
    let data = state
        .downcast_ref::<RouterData>()
        .ok_or_else(|| "invalid Phera\\Http\\Router state".to_owned())?;
    let handlers = data.handlers.iter().map(|v| fork.value(v)).collect();
    let fallback = data.fallback.as_ref().map(|v| fork.value(v));
    let on_start = data.on_start.iter().map(|v| fork.value(v)).collect();
    Ok(Box::new(RouterData {
        router: Rc::clone(&data.router),
        handlers,
        fallback,
        on_start,
        frozen: data.frozen,
    }))
}

fn trace_router(state: &dyn Any, visit: &mut dyn FnMut(&Value)) -> bool {
    let Some(data) = state.downcast_ref::<RouterData>() else {
        return false;
    };
    for handler in &data.handlers {
        visit(handler);
    }
    if let Some(fallback) = &data.fallback {
        visit(fallback);
    }
    for cb in &data.on_start {
        visit(cb);
    }
    true
}

pub(super) fn register(registry: &mut Registry) {
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
        MethodEntry::public("__construct", &[], construct),
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
        MethodEntry::public("onStart", CALLBACK, on_start).returning("static"),
    ];
    const HOOKS: ClassHooks = ClassHooks {
        fork: Some(fork_router),
        trace_native: Some(trace_router),
        forbid_serialization: true,
        ..ClassHooks::EMPTY
    };
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        methods: METHODS,
        hooks: Some(&HOOKS),
        ..ClassEntry::class(ROUTER_CLASS)
    });

    const SERVE_PARAMS: &[Param] = &[
        Param::new("router", ParamType::Class(ROUTER_CLASS)),
        Param::new("addr", ParamType::String).with_default(ConstantValue::String("127.0.0.1:8080")),
        Param::new("workers", ParamType::Int).with_default(ConstantValue::Int(0)),
    ];
    registry.function(FunctionEntry::sync(SERVE_FUNCTION, SERVE_PARAMS, serve_fn));
}

fn this_object(this: Option<&Rc<Object>>) -> Result<&Rc<Object>, NativeError> {
    this.ok_or_else(|| NativeError::error("Phera\\Http\\Router method called without an object"))
}

fn with_router<R>(
    this: Option<&Rc<Object>>,
    change: impl FnOnce(&mut RouterData) -> Result<R, NativeError>,
) -> Result<R, NativeError> {
    let object = this_object(this)?;
    let mut native = object.native.borrow_mut();
    let data = native
        .as_mut()
        .and_then(|native| native.downcast_mut::<RouterData>())
        .ok_or_else(|| NativeError::error("Phera\\Http\\Router object is not initialized"))?;
    change(data)
}

fn construct(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let object = this_object(this)?;
    if object.native.borrow().is_some() {
        return Err(NativeError::throw(
            "LogicException",
            "Phera\\Http\\Router::__construct() cannot be called twice",
        ));
    }
    *object.native.borrow_mut() = Some(Box::new(RouterData {
        router: Rc::new(Router::new()),
        handlers: Vec::new(),
        fallback: None,
        on_start: Vec::new(),
        frozen: false,
    }));
    Ok(Value::Null)
}

fn refuse_when_frozen(data: &RouterData) -> Result<(), NativeError> {
    if data.frozen {
        return Err(NativeError::throw(
            "LogicException",
            "Phera\\Http\\Router: routes cannot change after serving starts",
        ));
    }
    Ok(())
}

fn add_route(
    this: Option<&Rc<Object>>,
    method: MethodFilter,
    path: &[u8],
    handler: &Value,
) -> NativeResult {
    let path = std::str::from_utf8(path).map_err(|_| {
        NativeError::value_error("Phera\\Http\\Router: a route path must be valid UTF-8")
    })?;
    with_router(this, |data| {
        refuse_when_frozen(data)?;
        let router = Rc::get_mut(&mut data.router).ok_or_else(|| {
            NativeError::throw(
                "LogicException",
                "Phera\\Http\\Router: routes cannot change after serving starts",
            )
        })?;
        let route = router.add(method, path).map_err(|error| match error {
            RouteError::InvalidPattern(message) => NativeError::value_error(message),
            RouteError::Duplicate => {
                let method = match method {
                    MethodFilter::Only(method) => method.as_str(),
                    MethodFilter::Any => "ANY",
                };
                NativeError::throw(
                    "LogicException",
                    format!("Phera\\Http\\Router: route {method} {path} is already registered"),
                )
            }
        })?;
        debug_assert_eq!(route as usize, data.handlers.len());
        data.handlers.push(handler.deref().clone());
        Ok(())
    })?;
    Ok(Value::Object(this_object(this)?.clone()))
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

fn route(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let name = String::from_utf8_lossy(args::bytes(arguments, 0)).into_owned();
    let method = if name.eq_ignore_ascii_case("ANY") || name == "*" {
        MethodFilter::Any
    } else {
        let method = Method::from_registration(&name).ok_or_else(|| {
            NativeError::value_error(format!(
                "Phera\\Http\\Router::route(): Argument #1 ($method) must be an HTTP method such as \"GET\" or \"ANY\", \"{name}\" given"
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
    with_router(this, |data| {
        refuse_when_frozen(data)?;
        data.fallback = Some(args::value(arguments, 0).deref().clone());
        Ok(())
    })?;
    Ok(Value::Object(this_object(this)?.clone()))
}

fn on_start(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    with_router(this, |data| {
        refuse_when_frozen(data)?;
        data.on_start
            .push(args::value(arguments, 0).deref().clone());
        Ok(())
    })?;
    Ok(Value::Object(this_object(this)?.clone()))
}

/// `"127.0.0.1:8080"`, `"[::1]:8080"`, `"localhost:8080"`, or `":8080"`.
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

/// `Phera\Http\serve(Router $router, string $addr = '127.0.0.1:8080', int $workers = 0): void`.
///
/// Under `phera serve`, captures `$router` and ends the boot cleanly.
/// Under `phera app.php`, records the target address/workers and exits so the
/// CLI hands off straight to the unified `phera serve` engine.
fn serve_fn(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let router_obj = match args::value(arguments, 0).deref().clone() {
        Value::Object(obj)
            if obj
                .native
                .borrow()
                .as_ref()
                .is_some_and(|n| n.is::<RouterData>()) =>
        {
            obj
        }
        _ => {
            return Err(NativeError::type_error(
                "Phera\\Http\\serve(): Argument #1 ($router) must be of type Phera\\Http\\Router",
            ));
        }
    };
    let text = String::from_utf8_lossy(args::bytes(arguments, 1)).into_owned();
    let addr = parse_address(&text).ok_or_else(|| {
        NativeError::value_error(format!(
            "Phera\\Http\\serve(): Argument #2 ($addr) must be \"host:port\", \"{text}\" given"
        ))
    })?;
    let workers = match args::int(arguments, 2).unwrap_or(0) {
        0 => None,
        count if count > 0 => Some(count as usize),
        _ => {
            return Err(NativeError::value_error(
                "Phera\\Http\\serve(): Argument #3 ($workers) must be greater than or equal to 0",
            ));
        }
    };

    let boot_slot = context.state().get_or_default::<BootRouterSlot>();
    if boot_slot.active {
        boot_slot.captured = Some(router_obj);
        return Err(context.exit_script());
    }

    context.state().get_or_default::<PendingServeSlot>().0 = Some(PendingServe { addr, workers });
    Err(context.exit_script())
}
