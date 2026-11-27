//! `Phera\Http\Client`: HTTP/1.1 and HTTP/2 requests over pooled
//! connections, answered with the same `Phera\Http\Response` a router
//! handler returns.
//!
//! ```php
//! use Phera\Http\Client;
//!
//! $api = new Client(['base_uri' => 'https://api.example.com/v1/', 'timeout' => 5.0,
//!                    'headers' => ['Authorization' => "Bearer $token"]]);
//! $user = $api->get('users/7', ['query' => ['fields' => 'name']])->decodeJson();
//! $created = $api->post('users', ['json' => ['name' => 'Ada']])->status();   // 201
//!
//! // Several requests at once; the answers keep the keys.
//! ['a' => $a, 'b' => $b] = $api->pool([
//!     'a' => ['method' => 'GET', 'uri' => 'slow/1'],
//!     'b' => ['method' => 'GET', 'uri' => 'slow/2'],
//! ]);
//! ```
//!
//! A request waits without blocking the thread: in `phera serve`, the
//! worker's other requests run meanwhile. Connections stay open between
//! requests (and across requests of a server), so a client made at boot or
//! in `$router->onStart()` reuses them. A connection, TLS or timeout
//! failure throws `Phera\Http\ClientException`; an HTTP error status is a
//! response like any other.

mod transport;

use std::rc::Rc;
use std::time::Duration;

use bytes::Bytes;
use http::header::{CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use http::{HeaderMap, Method};
use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, ClassKind, ConstantValue, Context, MethodEntry, NativeError,
    NativeResult, Object, Param, ParamType, PhpArray, PhpString, Registry, Value, args, fork_clone,
};
use url::Url;

use transport::{ClientError, Prepared, Received, Transport};

const CLIENT_CLASS: &str = "Phera\\Http\\Client";
const EXCEPTION_CLASS: &str = "Phera\\Http\\ClientException";

/// What a `Client` was constructed with. It never changes, so a fork of a
/// snapshot shares it; connections live in the thread's pool.
struct Config {
    base_uri: Option<Url>,
    timeout: Option<Duration>,
    headers: HeaderMap,
    max_redirects: u32,
    transport: Transport,
}

pub(crate) fn register(registry: &mut Registry) {
    const CONSTRUCT: &[Param] =
        &[Param::new("options", ParamType::Array).with_default(ConstantValue::EmptyArray)];
    const OPTIONS: Param =
        Param::new("options", ParamType::Array).with_default(ConstantValue::EmptyArray);
    const REQUEST: &[Param] = &[
        Param::new("method", ParamType::String),
        Param::new("uri", ParamType::String),
        OPTIONS,
    ];
    const VERB: &[Param] = &[Param::new("uri", ParamType::String), OPTIONS];
    const POOL: &[Param] = &[Param::new("requests", ParamType::Array)];

    /// `$client->get($uri, $options)` and the other verbs.
    macro_rules! verb {
        ($name:literal, $method:expr) => {
            MethodEntry::public_async($name, VERB, |context, this, arguments| {
                Box::pin(request_with(context, this, arguments, Some($method)))
            })
            .returning(RESPONSE)
        };
    }
    const RESPONSE: &str = "Phera\\Http\\Response";
    const METHODS: &[MethodEntry] = &[
        MethodEntry::public("__construct", CONSTRUCT, construct),
        MethodEntry::public_async("request", REQUEST, |context, this, arguments| {
            Box::pin(request_with(context, this, arguments, None))
        })
        .returning(RESPONSE),
        verb!("get", Method::GET),
        verb!("post", Method::POST),
        verb!("put", Method::PUT),
        verb!("patch", Method::PATCH),
        verb!("delete", Method::DELETE),
        verb!("head", Method::HEAD),
        MethodEntry::public_async("pool", POOL, |context, this, arguments| {
            Box::pin(pool(context, this, arguments))
        })
        .returning("array"),
    ];
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        methods: METHODS,
        hooks: Some(&HOOKS),
        ..ClassEntry::class(CLIENT_CLASS)
    });
    registry.class(ClassEntry {
        parent: Some("RuntimeException"),
        ..ClassEntry::class(EXCEPTION_CLASS)
    });
}

/// A client made before `Phera\snapshot()` works in every request.
const HOOKS: ClassHooks = ClassHooks {
    fork: Some(fork_clone::<Rc<Config>>),
    forbid_serialization: true,
    ..ClassHooks::EMPTY
};

fn config(this: Option<&Rc<Object>>) -> Result<Rc<Config>, NativeError> {
    this.and_then(|object| {
        object
            .native
            .borrow()
            .as_ref()
            .and_then(|native| native.downcast_ref::<Rc<Config>>())
            .cloned()
    })
    .ok_or_else(|| NativeError::error("Phera\\Http\\Client is not initialized"))
}

fn exception(error: ClientError) -> NativeError {
    NativeError::throw(EXCEPTION_CLASS, error.0)
}

fn invalid_option(name: &str, expected: &str) -> NativeError {
    NativeError::value_error(format!(
        "Phera\\Http\\Client: option \"{name}\" must be {expected}"
    ))
}

/// The options of an array, as `(name, value)`; integer keys are refused.
fn options(array: &PhpArray) -> Result<Vec<(String, Value)>, NativeError> {
    array
        .iter()
        .map(|(key, value)| match key.as_str_bytes() {
            Some(name) => Ok((
                String::from_utf8_lossy(name).into_owned(),
                value.deref().clone(),
            )),
            None => Err(NativeError::value_error(
                "Phera\\Http\\Client: options must have names as keys",
            )),
        })
        .collect()
}

/// Seconds as an int or a float; `0` means no limit.
fn seconds(name: &str, value: &Value) -> Result<Option<Duration>, NativeError> {
    let seconds = match value {
        Value::Int(seconds) => *seconds as f64,
        Value::Float(seconds) => *seconds,
        _ => return Err(invalid_option(name, "a number of seconds")),
    };
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(invalid_option(name, "a number of seconds"));
    }
    Ok((seconds > 0.0).then(|| Duration::from_secs_f64(seconds)))
}

fn flag(name: &str, value: &Value) -> Result<bool, NativeError> {
    match value {
        Value::Bool(flag) => Ok(*flag),
        _ => Err(invalid_option(name, "a bool")),
    }
}

fn redirects(name: &str, value: &Value) -> Result<u32, NativeError> {
    match value {
        Value::Int(count) => u32::try_from(*count).map_err(|_| invalid_option(name, "0 or more")),
        _ => Err(invalid_option(name, "an int")),
    }
}

/// `['Accept' => 'text/html', 'X-Tag' => ['a', 'b']]` over `headers`: each
/// name given replaces that header's lines.
fn apply_headers(headers: &mut HeaderMap, value: &Value) -> Result<(), NativeError> {
    let Value::Array(array) = value else {
        return Err(invalid_option("headers", "an array of name => value"));
    };
    for (key, value) in array.iter() {
        let name = key
            .as_str_bytes()
            .and_then(|name| HeaderName::from_bytes(name).ok())
            .ok_or_else(|| {
                NativeError::value_error(format!(
                    "Phera\\Http\\Client: \"{}\" is not a valid header name",
                    key.as_str_bytes()
                        .map(String::from_utf8_lossy)
                        .unwrap_or_default()
                ))
            })?;
        headers.remove(&name);
        let value = value.deref();
        let lines: Vec<&Value> = match &*value {
            Value::Array(lines) => lines.values().collect(),
            single => vec![single],
        };
        for line in lines {
            let text = match &*line.deref() {
                Value::Int(number) => number.to_string().into_bytes(),
                other => other
                    .as_str_bytes()
                    .ok_or_else(|| invalid_option("headers", "an array of strings"))?
                    .to_vec(),
            };
            let line = HeaderValue::from_bytes(&text).map_err(|_| {
                NativeError::value_error(format!(
                    "Phera\\Http\\Client: the value of header \"{name}\" must not contain CR, LF or NUL"
                ))
            })?;
            headers.append(name.clone(), line);
        }
    }
    Ok(())
}

fn construct(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let Some(object) = this else {
        return Ok(Value::Null);
    };
    let mut config = Config {
        base_uri: None,
        timeout: Some(Duration::from_secs(30)),
        headers: HeaderMap::new(),
        max_redirects: 5,
        transport: Transport {
            verify: true,
            http2: true,
            http2_prior_knowledge: false,
            connect_timeout: Some(Duration::from_secs(10)),
        },
    };
    if let Some(given) = args::array(arguments, 0) {
        for (name, value) in options(given)? {
            match name.as_str() {
                "base_uri" => {
                    let text = value
                        .as_str_bytes()
                        .ok_or_else(|| invalid_option(&name, "a string"))?;
                    config.base_uri = Some(absolute_url(&String::from_utf8_lossy(text))?);
                }
                "timeout" => config.timeout = seconds(&name, &value)?,
                "connect_timeout" => config.transport.connect_timeout = seconds(&name, &value)?,
                "headers" => apply_headers(&mut config.headers, &value)?,
                "max_redirects" => config.max_redirects = redirects(&name, &value)?,
                "verify" => config.transport.verify = flag(&name, &value)?,
                "http2" => config.transport.http2 = flag(&name, &value)?,
                "http2_prior_knowledge" => {
                    config.transport.http2_prior_knowledge = flag(&name, &value)?
                }
                _ => return Err(unknown_option(&name)),
            }
        }
    }
    if !config.headers.contains_key(USER_AGENT) {
        config.headers.insert(
            USER_AGENT,
            HeaderValue::from_static(concat!("Phera/", env!("CARGO_PKG_VERSION"))),
        );
    }
    *object.native.borrow_mut() = Some(Box::new(Rc::new(config)));
    Ok(Value::Null)
}

fn unknown_option(name: &str) -> NativeError {
    NativeError::value_error(format!("Phera\\Http\\Client: unknown option \"{name}\""))
}

/// An `http://` or `https://` URL.
fn absolute_url(text: &str) -> Result<Url, NativeError> {
    let url = Url::parse(text).map_err(|error| {
        NativeError::value_error(format!(
            "Phera\\Http\\Client: \"{text}\" is not an absolute URL: {error}"
        ))
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(NativeError::value_error(format!(
            "Phera\\Http\\Client: \"{text}\" is not an http:// or https:// URL"
        )));
    }
    Ok(url)
}

/// `$uri` against `base_uri`, as RFC 3986 resolves a reference.
fn target(config: &Config, uri: &str) -> Result<Url, NativeError> {
    match &config.base_uri {
        Some(base) => {
            let url = base.join(uri).map_err(|error| {
                NativeError::value_error(format!(
                    "Phera\\Http\\Client: \"{uri}\" is not a valid URI: {error}"
                ))
            })?;
            absolute_url(url.as_str())
        }
        None => absolute_url(uri),
    }
}

/// One request's method, URL, headers and body from the PHP arguments.
async fn prepare(
    context: &mut dyn Context,
    config: &Config,
    method: &[u8],
    uri: &[u8],
    given: Option<&PhpArray>,
) -> Result<Prepared, NativeError> {
    let method = Method::from_bytes(method.to_ascii_uppercase().as_slice()).map_err(|_| {
        NativeError::value_error(format!(
            "Phera\\Http\\Client: \"{}\" is not a valid HTTP method",
            String::from_utf8_lossy(method)
        ))
    })?;
    let mut request = Prepared {
        method,
        url: target(config, &String::from_utf8_lossy(uri))?,
        headers: config.headers.clone(),
        body: Bytes::new(),
        timeout: config.timeout,
        max_redirects: config.max_redirects,
    };
    let Some(given) = given else {
        return Ok(request);
    };
    let mut has_body = false;
    for (name, value) in options(given)? {
        match name.as_str() {
            "headers" => apply_headers(&mut request.headers, &value)?,
            "query" => {
                let query = match &value {
                    Value::Array(_) => {
                        context
                            .call(&Value::from("http_build_query"), vec![value.clone()])
                            .await?
                    }
                    Value::String(_) | Value::InternedStr(_) => value.clone(),
                    _ => return Err(invalid_option(&name, "an array or a string")),
                };
                let query =
                    String::from_utf8_lossy(query.as_str_bytes().unwrap_or_default()).into_owned();
                request
                    .url
                    .set_query((!query.is_empty()).then_some(query.as_str()));
            }
            "body" | "json" if has_body => {
                return Err(NativeError::value_error(
                    "Phera\\Http\\Client: give either \"body\" or \"json\", not both",
                ));
            }
            "body" => {
                let body = value
                    .as_str_bytes()
                    .ok_or_else(|| invalid_option(&name, "a string"))?;
                request.body = Bytes::copy_from_slice(body);
                has_body = true;
            }
            "json" => {
                let body = crate::http_server::encode_json(context, &value).await?;
                request.body = Bytes::from(body);
                has_body = true;
                if !request.headers.contains_key(CONTENT_TYPE) {
                    request
                        .headers
                        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }
            }
            "timeout" => request.timeout = seconds(&name, &value)?,
            "max_redirects" => request.max_redirects = redirects(&name, &value)?,
            _ => return Err(unknown_option(&name)),
        }
    }
    Ok(request)
}

/// The `Phera\Http\Response` for what came back.
fn response(context: &mut dyn Context, received: Received) -> NativeResult {
    let mut headers = Vec::with_capacity(received.headers.len());
    for (name, value) in &received.headers {
        headers.push((name.as_str().to_owned(), PhpString::from(value.as_bytes())));
    }
    crate::http_server::received(
        context,
        received.status,
        headers,
        PhpString::from(received.body.to_vec()),
    )
}

/// `request($method, $uri, $options)`, or a verb's `get($uri, $options)`.
async fn request_with(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    verb: Option<Method>,
) -> NativeResult {
    let config = config(this.as_ref())?;
    let (method, rest) = match &verb {
        Some(method) => (method.as_str().as_bytes(), 0),
        None => (args::bytes(&arguments, 0), 1),
    };
    let request = prepare(
        context,
        &config,
        method,
        args::bytes(&arguments, rest),
        args::array(&arguments, rest + 1).map(|options| &**options),
    )
    .await?;
    let client = transport::client(config.transport)
        .await
        .map_err(exception)?;
    let received = transport::send(client, request).await.map_err(exception)?;
    response(context, received)
}

/// Sends every request at once and waits for all of them:
/// `[$key => ['method' => 'GET', 'uri' => '/a', ...options]]` gives
/// `[$key => Response]`. If any fails, `ClientException` names its key.
async fn pool(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> NativeResult {
    let config = config(this.as_ref())?;
    let empty = Rc::new(PhpArray::new());
    let requests = args::array(&arguments, 0).unwrap_or(&empty);
    let mut keys = Vec::with_capacity(requests.len());
    let mut prepared = Vec::with_capacity(requests.len());
    for (key, entry) in requests.iter() {
        let Value::Array(entry) = &*entry.deref() else {
            return Err(NativeError::value_error(
                "Phera\\Http\\Client::pool(): each request must be an array with \"uri\" (and \"method\", options)",
            ));
        };
        let mut method = b"GET".to_vec();
        let mut uri = None;
        let mut options = PhpArray::new();
        for (name, value) in entry.iter() {
            match name.as_str_bytes() {
                Some(b"method") => method = value.as_str_bytes().unwrap_or(b"GET").to_vec(),
                Some(b"uri") => uri = value.as_str_bytes().map(<[u8]>::to_vec),
                _ => {
                    options.insert(name, value.deref().clone());
                }
            }
        }
        let uri = uri.ok_or_else(|| {
            NativeError::value_error(format!(
                "Phera\\Http\\Client::pool(): request {} has no \"uri\"",
                key_text(&key)
            ))
        })?;
        prepared.push(prepare(context, &config, &method, &uri, Some(&options)).await?);
        keys.push(key);
    }
    let client = transport::client(config.transport)
        .await
        .map_err(exception)?;
    let sends = prepared
        .into_iter()
        .map(|request| transport::send(client.clone(), request));
    let results = futures_util::future::join_all(sends).await;
    let mut responses = PhpArray::with_capacity(keys.len());
    for (key, result) in keys.into_iter().zip(results) {
        let received = result.map_err(|error| {
            exception(ClientError(format!(
                "pool() request {} failed: {}",
                key_text(&key),
                error.0
            )))
        })?;
        responses.insert(key, response(context, received)?);
    }
    Ok(Value::from(responses))
}

fn key_text(key: &ArrayKey) -> String {
    match key {
        ArrayKey::Int(index) => index.to_string(),
        other => format!(
            "\"{}\"",
            String::from_utf8_lossy(other.as_str_bytes().unwrap_or_default())
        ),
    }
}
