//! `Phera\Grpc\Context`, a server handler's view of its call.
//!
//! ```php
//! $server->unary('/users.Users/Get', function (array $req, Context $ctx) {
//!     $user = $ctx->header('x-user');           // request metadata
//!     $ctx->setTrailer('x-served-by', 'node-1'); // sent after the response
//!     return ['name' => $user, 'budget' => $ctx->deadlineMs()];
//! }, 'users.GetRequest', 'users.User');
//! ```

use std::cell::{Cell, RefCell};
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Instant;

use http::HeaderMap;
use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, Context, MethodEntry, NativeError, NativeResult, Object,
    Param, ParamType, PhpArray, Registry, Value, args, fork_refused_handle,
};

use crate::headers;

pub const CONTEXT_CLASS: &str = "Phera\\Grpc\\Context";

/// What a handler can read about its call, and the metadata it adds.
pub struct CallState {
    /// `/pkg.Service/Method`.
    pub method: Box<str>,
    pub request_headers: HeaderMap,
    pub deadline: Option<Instant>,
    pub peer: Option<SocketAddr>,
    pub response_headers: RefCell<HeaderMap>,
    pub trailers: RefCell<HeaderMap>,
    /// Set once the response headers are on their way.
    pub headers_sent: Cell<bool>,
}

pub fn register(registry: &mut Registry) {
    const NAME: &[Param] = &[Param::new("name", ParamType::String)];
    const NAME_VALUE: &[Param] = &[
        Param::new("name", ParamType::String),
        Param::new("value", ParamType::String),
    ];
    const METHODS: &[MethodEntry] = &[
        MethodEntry::public("method", &[], method).returning("string"),
        MethodEntry::public("metadata", &[], metadata).returning("array"),
        MethodEntry::public("header", NAME, header).returning("?string"),
        MethodEntry::public("setHeader", NAME_VALUE, set_header).returning("void"),
        MethodEntry::public("setTrailer", NAME_VALUE, set_trailer).returning("void"),
        MethodEntry::public("deadlineMs", &[], deadline_ms).returning("?int"),
        MethodEntry::public("peer", &[], peer).returning("string"),
    ];
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        methods: METHODS,
        hooks: Some(&HOOKS),
        ..ClassEntry::class(CONTEXT_CLASS)
    });
}

const HOOKS: ClassHooks = ClassHooks {
    construction_error: Some("Cannot directly construct Phera\\Grpc\\Context"),
    fork: Some(fork_refused_handle),
    forbid_serialization: true,
    ..ClassHooks::EMPTY
};

/// A `Context` object for one call.
pub fn create(context: &mut dyn Context, state: Rc<CallState>) -> NativeResult {
    let object = context.create_object(CONTEXT_CLASS, PhpArray::new())?;
    if let Value::Object(object) = &object {
        *object.native.borrow_mut() = Some(Box::new(state));
    }
    Ok(object)
}

fn state(this: Option<&Rc<Object>>) -> Result<Rc<CallState>, NativeError> {
    this.and_then(|object| {
        object
            .native
            .borrow()
            .as_ref()
            .and_then(|native| native.downcast_ref::<Rc<CallState>>())
            .cloned()
    })
    .ok_or_else(|| NativeError::error("Phera\\Grpc\\Context is not initialized"))
}

fn method(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::from(&*state(this)?.method))
}

fn metadata(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::from(headers::to_array(
        &state(this)?.request_headers,
    )))
}

/// Any request header by name, including reserved ones such as
/// `grpc-timeout`; `-bin` values decoded; several values joined by `,`.
fn header(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let state = state(this)?;
    let name = String::from_utf8_lossy(args::bytes(arguments, 0)).to_ascii_lowercase();
    let mut values = state
        .request_headers
        .get_all(name.as_str())
        .iter()
        .peekable();
    if values.peek().is_none() {
        return Ok(Value::Null);
    }
    let mut joined = Vec::new();
    for (index, value) in values.enumerate() {
        if index > 0 && !name.ends_with("-bin") {
            joined.push(b',');
        }
        joined.extend_from_slice(&headers::header_bytes(&name, value));
    }
    Ok(Value::from(joined))
}

fn set_header(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let state = state(this)?;
    if state.headers_sent.get() {
        return Err(NativeError::throw(
            "LogicException",
            "Response headers were already sent; set them before the first message",
        ));
    }
    add(&mut state.response_headers.borrow_mut(), arguments)
}

fn set_trailer(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    add(&mut state(this)?.trailers.borrow_mut(), arguments)
}

fn add(target: &mut HeaderMap, arguments: &[Value]) -> NativeResult {
    let name = String::from_utf8_lossy(args::bytes(arguments, 0)).into_owned();
    headers::insert(target, &name, args::value(arguments, 1)).map_err(NativeError::value_error)?;
    Ok(Value::Null)
}

/// Milliseconds left before the client's deadline (0 once it has passed),
/// or null when the client set none.
fn deadline_ms(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(match state(this)?.deadline {
        Some(deadline) => {
            let left = deadline.saturating_duration_since(Instant::now());
            Value::Int(i64::try_from(left.as_millis()).unwrap_or(i64::MAX))
        }
        None => Value::Null,
    })
}

fn peer(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::from(
        state(this)?
            .peer
            .map(|address| address.to_string())
            .unwrap_or_default(),
    ))
}
