//! The `Phera\Http\Response` class, and what a handler's return value
//! becomes on the wire.
//!
//! ```php
//! return Response::json(['id' => 7], 201, ['Location' => '/users/7']);
//! return Response::text('pong')->withHeader('Cache-Control', 'no-store');
//! return 'Hello';               // 200, text/plain
//! return ['ok' => true];        // 200, application/json
//! return null;                  // 204, no body
//! ```
//!
//! A response is plain Rust data inside the object: a status, header lines
//! and the body bytes. `header()` and output buffers have no part in it.
//! `with*()` methods return a changed copy, as in PSR-7; the body is shared
//! between the copies, not copied.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassKind, ConstantValue, Context, LocalFuture, MethodEntry, NativeError,
    NativeResult, Object, Param, ParamType, PhpArray, PhpString, Registry, Value, Visibility, args,
};
use phera_server::{Body, Response};
use smallvec::SmallVec;

use super::RESPONSE_CLASS;

const TEXT: &str = "text/plain; charset=utf-8";
const HTML: &str = "text/html; charset=utf-8";
const JSON: &str = "application/json";

/// `json_encode` flags of `Response::json()`: slashes and Unicode as they
/// are (as `JSON.stringify` writes them), and an exception on failure.
const JSON_FLAGS: i64 = phera_ext_json::flag::UNESCAPED_SLASHES
    | phera_ext_json::flag::UNESCAPED_UNICODE
    | phera_ext_json::flag::THROW_ON_ERROR;

/// The native state of a `Phera\Http\Response` object.
#[derive(Clone)]
struct ResponseData {
    status: u16,
    /// Lower-case names, one entry per header line, in the order set.
    headers: SmallVec<[(String, PhpString); 4]>,
    body: PhpString,
}

impl ResponseData {
    fn new(status: u16, content_type: Option<&str>, body: PhpString) -> ResponseData {
        let mut headers = SmallVec::new();
        if let Some(content_type) = content_type {
            headers.push(("content-type".to_owned(), PhpString::from(content_type)));
        }
        ResponseData {
            status,
            headers,
            body,
        }
    }

    /// Replaces every line of header `name` with one line of `value`.
    fn set_header(&mut self, name: String, value: PhpString) {
        self.headers.retain(|(existing, _)| *existing != name);
        self.headers.push((name, value));
    }

    fn into_server(self) -> Response {
        Response {
            status: self.status,
            headers: self
                .headers
                .into_iter()
                .map(|(name, value)| (name, value.into_bytes()))
                .collect(),
            body: Body::Bytes(self.body.into_bytes()),
            upgrade: None,
        }
    }
}

pub(super) fn register(registry: &mut Registry) {
    const STATUS_DEFAULT: Param =
        Param::new("status", ParamType::Int).with_default(ConstantValue::Int(200));
    const HEADERS_DEFAULT: Param =
        Param::new("headers", ParamType::Array).with_default(ConstantValue::EmptyArray);
    const BODY: &[Param] = &[
        Param::new("body", ParamType::String),
        STATUS_DEFAULT,
        HEADERS_DEFAULT,
    ];
    const DATA: &[Param] = &[
        Param::new("data", ParamType::Mixed),
        STATUS_DEFAULT,
        HEADERS_DEFAULT,
    ];
    const BYTES: &[Param] = &[
        Param::new("body", ParamType::String),
        Param::new("contentType", ParamType::String)
            .with_default(ConstantValue::String("application/octet-stream")),
        STATUS_DEFAULT,
        HEADERS_DEFAULT,
    ];
    const EMPTY: &[Param] = &[
        Param::new("status", ParamType::Int).with_default(ConstantValue::Int(204)),
        HEADERS_DEFAULT,
    ];
    const STATUS: &[Param] = &[Param::new("status", ParamType::Int)];
    const HEADER: &[Param] = &[
        Param::new("name", ParamType::String),
        Param::new("value", ParamType::String),
    ];
    const NAME: &[Param] = &[Param::new("name", ParamType::String)];
    const METHODS: &[MethodEntry] = &[
        MethodEntry::public("__construct", &[], private_construct)
            .with_visibility(Visibility::Private),
        MethodEntry::public("text", BODY, text)
            .static_()
            .returning("static"),
        MethodEntry::public("html", BODY, html)
            .static_()
            .returning("static"),
        MethodEntry::public_async("json", DATA, json)
            .static_()
            .returning("static"),
        MethodEntry::public("bytes", BYTES, bytes)
            .static_()
            .returning("static"),
        MethodEntry::public("empty", EMPTY, empty)
            .static_()
            .returning("static"),
        MethodEntry::public("withStatus", STATUS, with_status).returning("static"),
        MethodEntry::public("withHeader", HEADER, with_header).returning("static"),
        MethodEntry::public("status", &[], status).returning("int"),
        MethodEntry::public("header", NAME, header).returning("?string"),
        MethodEntry::public("headers", &[], headers).returning("array"),
        MethodEntry::public("body", &[], body).returning("string"),
    ];
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        methods: METHODS,
        ..ClassEntry::class(RESPONSE_CLASS)
    });
}

/// Responses are made by the static constructors only.
fn private_construct(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    Ok(Value::Null)
}

fn new_object(context: &mut dyn Context, data: ResponseData) -> NativeResult {
    let object = context.create_object(RESPONSE_CLASS, PhpArray::new())?;
    if let Value::Object(object) = &object {
        *object.native.borrow_mut() = Some(Box::new(data));
    }
    Ok(object)
}

fn with_response<R>(
    this: Option<&Rc<Object>>,
    read: impl FnOnce(&ResponseData) -> R,
) -> Result<R, NativeError> {
    let object = this.ok_or_else(|| {
        NativeError::error("Phera\\Http\\Response method called without an object")
    })?;
    let native = object.native.borrow();
    let data = native
        .as_ref()
        .and_then(|native| native.downcast_ref::<ResponseData>())
        .ok_or_else(|| NativeError::error("Phera\\Http\\Response object is not initialized"))?;
    Ok(read(data))
}

fn string_argument(arguments: &[Value], index: usize) -> PhpString {
    match args::value(arguments, index) {
        Value::String(string) => string.clone(),
        other => PhpString::from(other.as_str_bytes().unwrap_or_default()),
    }
}

fn status_argument(arguments: &[Value], index: usize, method: &str) -> Result<u16, NativeError> {
    let status = args::int(arguments, index).unwrap_or(200);
    if !(100..=599).contains(&status) {
        return Err(NativeError::value_error(format!(
            "Phera\\Http\\Response::{method}(): Argument #{} ($status) must be between 100 and 599",
            index + 1
        )));
    }
    Ok(status as u16)
}

/// A response with `body`, `content_type`, and the status and headers at
/// `status_index` and `status_index + 1` of `arguments`.
fn build(
    context: &mut dyn Context,
    method: &str,
    body: PhpString,
    content_type: Option<&str>,
    arguments: &[Value],
    status_index: usize,
) -> NativeResult {
    let status = status_argument(arguments, status_index, method)?;
    let mut data = ResponseData::new(status, content_type, body);
    if let Some(headers) = args::array(arguments, status_index + 1) {
        apply_headers(&mut data, headers, method, status_index + 2)?;
    }
    new_object(context, data)
}

fn text(context: &mut dyn Context, _: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let body = string_argument(arguments, 0);
    build(context, "text", body, Some(TEXT), arguments, 1)
}

fn html(context: &mut dyn Context, _: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let body = string_argument(arguments, 0);
    build(context, "html", body, Some(HTML), arguments, 1)
}

fn bytes(context: &mut dyn Context, _: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let body = string_argument(arguments, 0);
    let content_type = String::from_utf8_lossy(args::bytes(arguments, 1)).into_owned();
    check_header_value(content_type.as_bytes(), "bytes")?;
    build(context, "bytes", body, Some(&content_type), arguments, 2)
}

fn empty(context: &mut dyn Context, _: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let mut arguments = arguments.to_vec();
    if arguments.is_empty() {
        arguments.push(Value::Int(204));
    }
    build(context, "empty", PhpString::new(), None, &arguments, 0)
}

fn json(
    context: &mut dyn Context,
    _: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let body = encode_json(context, args::value(&arguments, 0)).await?;
        build(
            context,
            "json",
            PhpString::from(body),
            Some(JSON),
            &arguments,
            1,
        )
    })
}

/// `json_encode($data, JSON_FLAGS)`: in Rust for plain values, through
/// `json_encode` for values that may run PHP (`JsonSerializable`).
async fn encode_json(context: &mut dyn Context, data: &Value) -> Result<Vec<u8>, NativeError> {
    let encoded = match phera_ext_json::encode_plain(context, data, JSON_FLAGS) {
        Some(encoded) => encoded?,
        None => {
            context
                .call(
                    &Value::from("json_encode"),
                    vec![data.clone(), Value::Int(JSON_FLAGS)],
                )
                .await?
        }
    };
    match encoded {
        Value::String(string) => Ok(string.into_bytes()),
        other => Ok(other.as_str_bytes().unwrap_or_default().to_vec()),
    }
}

fn with_status(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let status = status_argument(arguments, 0, "withStatus")?;
    let mut data = with_response(this, ResponseData::clone)?;
    data.status = status;
    new_object(context, data)
}

fn with_header(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let name = header_name(args::bytes(arguments, 0), "withHeader")?;
    let value = string_argument(arguments, 1);
    check_header_value(value.as_bytes(), "withHeader")?;
    let mut data = with_response(this, ResponseData::clone)?;
    data.set_header(name, value);
    new_object(context, data)
}

fn status(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_response(this, |data| Value::Int(i64::from(data.status)))
}

/// One header's lines joined by `", "`, as PSR-7's `getHeaderLine()`.
fn header(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let name = String::from_utf8_lossy(args::bytes(arguments, 0)).to_ascii_lowercase();
    with_response(this, |data| {
        let mut lines = data
            .headers
            .iter()
            .filter(|(candidate, _)| *candidate == name)
            .map(|(_, value)| value.as_bytes());
        let Some(first) = lines.next() else {
            return Value::Null;
        };
        let mut joined = first.to_vec();
        for line in lines {
            joined.extend_from_slice(b", ");
            joined.extend_from_slice(line);
        }
        Value::from(joined)
    })
}

/// Every header as `name => [lines]`, as PSR-7's `getHeaders()`.
fn headers(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_response(this, |data| {
        let mut headers = PhpArray::new();
        for (name, value) in &data.headers {
            let key = ArrayKey::from_string(PhpString::from(name.as_str()));
            let mut lines = match headers.get(&key) {
                Some(Value::Array(lines)) => (**lines).clone(),
                _ => PhpArray::new(),
            };
            let _ = lines.push(Value::String(value.clone()));
            headers.insert(key, Value::from(lines));
        }
        Value::from(headers)
    })
}

fn body(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_response(this, |data| Value::String(data.body.clone()))
}

/// Applies a `['Name' => 'value']` array: each name replaces the lines of
/// that header (the default `Content-Type` too); a list of values sends one
/// line each (`'Set-Cookie' => ['a=1', 'b=2']`).
fn apply_headers(
    data: &mut ResponseData,
    headers: &PhpArray,
    method: &str,
    argument: usize,
) -> Result<(), NativeError> {
    for (key, value) in headers.iter() {
        let Some(name) = key.as_str_bytes() else {
            return Err(NativeError::value_error(format!(
                "Phera\\Http\\Response::{method}(): Argument #{argument} ($headers) must have header names as keys"
            )));
        };
        let name = header_name(name, method)?;
        let value = value.deref();
        let lines: SmallVec<[PhpString; 2]> = match &*value {
            Value::Array(lines) => lines
                .values()
                .map(|line| header_line(&line.deref(), method, argument))
                .collect::<Result<_, _>>()?,
            single => SmallVec::from_iter([header_line(single, method, argument)?]),
        };
        data.headers.retain(|(existing, _)| *existing != name);
        for line in lines {
            data.headers.push((name.clone(), line));
        }
    }
    Ok(())
}

fn header_line(value: &Value, method: &str, argument: usize) -> Result<PhpString, NativeError> {
    let line = match value {
        Value::String(string) => string.clone(),
        Value::InternedStr(string) => PhpString::from(string.as_bytes()),
        Value::Int(number) => PhpString::from(number.to_string()),
        _ => {
            return Err(NativeError::type_error(format!(
                "Phera\\Http\\Response::{method}(): Argument #{argument} ($headers) must hold strings, {} given",
                value.type_name()
            )));
        }
    };
    check_header_value(line.as_bytes(), method)?;
    Ok(line)
}

/// A header name as HTTP allows it (a token), lower-cased.
fn header_name(name: &[u8], method: &str) -> Result<String, NativeError> {
    let is_token_byte =
        |byte: &u8| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(byte);
    if name.is_empty() || !name.iter().all(is_token_byte) {
        return Err(NativeError::value_error(format!(
            "Phera\\Http\\Response::{method}(): \"{}\" is not a valid header name",
            String::from_utf8_lossy(name)
        )));
    }
    Ok(String::from_utf8_lossy(name).to_ascii_lowercase())
}

/// A header value must not end the header early: no CR, LF or NUL.
fn check_header_value(value: &[u8], method: &str) -> Result<(), NativeError> {
    if value.iter().any(|byte| matches!(byte, b'\r' | b'\n' | 0)) {
        return Err(NativeError::value_error(format!(
            "Phera\\Http\\Response::{method}(): a header value must not contain CR, LF or NUL"
        )));
    }
    Ok(())
}

/// What a handler's return value sends: a `Response` as it is, a string as
/// `text/plain`, an array as JSON, `null` as an empty 204.
pub(super) async fn from_handler_result(
    context: &mut dyn Context,
    result: Value,
) -> Result<Response, NativeError> {
    let result = result.deref().clone();
    match result {
        Value::Object(object) => match take_response(object) {
            Some(data) => Ok(data.into_server()),
            None => Err(wrong_return_type("an object of another class")),
        },
        Value::String(body) => Ok(ResponseData::new(200, Some(TEXT), body).into_server()),
        Value::InternedStr(body) => {
            Ok(ResponseData::new(200, Some(TEXT), PhpString::from(body.as_bytes())).into_server())
        }
        Value::Array(_) => {
            let body = encode_json(context, &result).await?;
            Ok(ResponseData::new(200, Some(JSON), PhpString::from(body)).into_server())
        }
        Value::Null => Ok(Response::new(204)),
        other => Err(wrong_return_type(&other.type_name())),
    }
}

/// The data of a `Response` object: moved out when the handler made it
/// for this request and nothing else holds it, copied otherwise.
fn take_response(object: Rc<Object>) -> Option<ResponseData> {
    if Rc::strong_count(&object) == 1 {
        let native = object.native.borrow_mut().take()?;
        return native.downcast::<ResponseData>().ok().map(|data| *data);
    }
    let native = object.native.borrow();
    native.as_ref()?.downcast_ref::<ResponseData>().cloned()
}

fn wrong_return_type(given: &str) -> NativeError {
    NativeError::type_error(format!(
        "Phera\\Http\\Server: a handler must return Phera\\Http\\Response, string, array or null, {given} returned"
    ))
}
