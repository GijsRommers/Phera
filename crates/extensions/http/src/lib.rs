//! The response side of a web request: `header()`, `http_response_code()`
//! and their relatives.
//!
//! PHP keeps the status code and the header list in global state that the
//! web server reads when the script ends. Here that state is a
//! [`ResponseHead`] stored in the request's [`RequestState`]: these functions
//! write it, and the runtime takes it out after the script has finished.
//!
//! [`RequestState`]: phera_extension_api::RequestState

mod cookies;
mod head;
mod log;
mod request;
mod uploads;

use phera_extension_api::{
    ConstantValue, Context, Extension, FunctionEntry, NativeResult, Param, ParamType, PhpArray,
    Registry, RequestCallback, Value, args,
};

pub use head::ResponseHead;
pub use request::{RequestHeaders, Sapi, register_web};
pub use uploads::UploadedFiles;

pub struct Http;

const HEADER_PARAMS: &[Param] = &[
    Param::new("header", ParamType::String),
    Param::new("replace", ParamType::Bool).with_default(ConstantValue::Bool(true)),
    Param::new("response_code", ParamType::Int).optional(),
];

const HEADER_REMOVE_PARAMS: &[Param] =
    &[Param::new("name", ParamType::String).nullable().optional()];

const HTTP_RESPONSE_CODE_PARAMS: &[Param] =
    &[Param::new("response_code", ParamType::Int).optional()];

const HEADERS_SENT_PARAMS: &[Param] = &[
    Param::new("filename", ParamType::Mixed).by_ref().optional(),
    Param::new("line", ParamType::Mixed).by_ref().optional(),
];

const HEADER_CALLBACK_PARAMS: &[Param] = &[Param::new("callback", ParamType::Callable)];
const SHUTDOWN_PARAMS: &[Param] = &[
    Param::new("callback", ParamType::Callable),
    Param::new("args", ParamType::Mixed).variadic(),
];

/// The values of `$_FILES[...]['error']`.
const UPLOAD_ERRORS: &[(&str, i64)] = &[
    ("UPLOAD_ERR_OK", 0),
    ("UPLOAD_ERR_INI_SIZE", 1),
    ("UPLOAD_ERR_FORM_SIZE", 2),
    ("UPLOAD_ERR_PARTIAL", 3),
    ("UPLOAD_ERR_NO_FILE", 4),
    ("UPLOAD_ERR_NO_TMP_DIR", 6),
    ("UPLOAD_ERR_CANT_WRITE", 7),
    ("UPLOAD_ERR_EXTENSION", 8),
];

impl Extension for Http {
    fn name(&self) -> &'static str {
        "http"
    }

    fn php_module(&self) -> Option<&'static str> {
        None
    }

    fn register(&self, registry: &mut Registry) {
        cookies::register(registry);
        log::register(registry);
        registry.function(FunctionEntry::sync(
            "php_sapi_name",
            &[],
            request::sapi_name,
        ));
        registry.function(FunctionEntry::asynchronous(
            "header_register_callback",
            HEADER_CALLBACK_PARAMS,
            |context, arguments| {
                context.register_request_callback(RequestCallback::Headers, arguments)
            },
        ));
        registry.function(FunctionEntry::asynchronous(
            "register_shutdown_function",
            SHUTDOWN_PARAMS,
            |context, arguments| {
                context.register_request_callback(RequestCallback::Shutdown, arguments)
            },
        ));
        registry.function(FunctionEntry::sync("header", HEADER_PARAMS, header));
        registry.function(FunctionEntry::sync(
            "header_remove",
            HEADER_REMOVE_PARAMS,
            header_remove,
        ));
        registry.function(FunctionEntry::sync("headers_list", &[], headers_list));
        registry.function(FunctionEntry::sync(
            "headers_sent",
            HEADERS_SENT_PARAMS,
            headers_sent,
        ));
        registry.function(FunctionEntry::sync(
            "http_response_code",
            HTTP_RESPONSE_CODE_PARAMS,
            http_response_code,
        ));
        uploads::register(registry);
        for (name, value) in UPLOAD_ERRORS {
            registry.constant(name, phera_extension_api::ConstantValue::Int(*value));
        }
    }
}

fn header(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let line = args::bytes(arguments, 0);
    let replace = args::bool(arguments, 1).unwrap_or(true);
    let status = args::int(arguments, 2).unwrap_or(0);

    let line = line.trim_ascii_end();
    if !headers_open(context) || !valid_header_line(context, line) {
        return Ok(Value::Null);
    }

    let head = context.state().get_or_default::<ResponseHead>();
    head.apply(line, replace);
    if let Ok(status @ 100..=599) = u16::try_from(status) {
        head.status = status;
    }
    Ok(Value::Null)
}

pub(crate) fn headers_open(context: &mut dyn Context) -> bool {
    let Some((file, line)) = context.output_origin() else {
        return true;
    };
    let message = if file.is_empty() {
        "Cannot modify header information - headers already sent".into()
    } else {
        format!(
            "Cannot modify header information - headers already sent by \
             (output started at {file}:{line})"
        )
    };
    context.warning(message);
    false
}

pub(crate) fn valid_header_line(context: &mut dyn Context, line: &[u8]) -> bool {
    for &byte in line {
        let message = match byte {
            0 => "Header may not contain NUL bytes",
            b'\r' | b'\n' => "Header may not contain more than a single header, new line detected",
            _ => continue,
        };
        context.warning(message.into());
        return false;
    }
    true
}

fn header_remove(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    if !headers_open(context) {
        return Ok(Value::Null);
    }
    let name = args::string(arguments, 0);
    if name.is_some_and(|name| name.as_bytes().contains(&b':')) {
        context.warning("Header to delete may not contain colon.".into());
        return Ok(Value::Null);
    }
    let head = context.state().get_or_default::<ResponseHead>();
    match name {
        Some(name) => head.remove(name.to_string_lossy().trim_end()),
        None => head.clear(),
    }
    Ok(Value::Null)
}

fn headers_list(context: &mut dyn Context, _arguments: &[Value]) -> NativeResult {
    let mut list = PhpArray::new();
    if matches!(request::sapi(context), Sapi::Web) {
        for line in context.state().get_or_default::<ResponseHead>().lines() {
            list.push(Value::from(line))?;
        }
    }
    Ok(Value::from(list))
}

fn headers_sent(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let origin = context.output_origin();
    let sent = origin.is_some();
    let (file, line) = origin.unwrap_or(("", 0));
    let file = Value::from(file);
    if let Some(reference) = args::reference(arguments, 1) {
        *reference.borrow_mut() = Value::Int(line as i64);
    }
    if let Some(reference) = args::reference(arguments, 0) {
        *reference.borrow_mut() = file;
    }
    Ok(Value::Bool(sent))
}

/// With an argument, sets the status and returns the previous one; without,
/// returns the current one.
fn http_response_code(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let head = context.state().get_or_default::<ResponseHead>();
    let previous = i64::from(head.status);
    match args::int(arguments, 0) {
        None | Some(0) => Ok(Value::Int(previous)),
        Some(requested) => {
            if let Ok(status @ 100..=599) = u16::try_from(requested) {
                head.status = status;
            }
            Ok(Value::Int(previous))
        }
    }
}

#[cfg(test)]
mod tests {
    use phera_extension_api::testing::{TestContext, call};

    use super::*;

    fn registry() -> Registry {
        let mut registry = Registry::new();
        Http.register(&mut registry);
        registry
    }

    fn send(context: &mut TestContext, registry: &Registry, line: &str) {
        call(registry, context, "header", vec![Value::from(line)]).expect("header");
    }

    #[test]
    fn headers_replace_by_name_and_a_location_redirects() {
        let registry = registry();
        let mut context = TestContext::default();
        send(&mut context, &registry, "X-Mode: one");
        send(&mut context, &registry, "x-mode: two");
        send(&mut context, &registry, "Location: /next");

        let head = context.state.take::<ResponseHead>().expect("head");
        assert_eq!(head.status, 302);
        assert_eq!(
            head.headers,
            vec![
                ("x-mode".to_string(), b"two".to_vec()),
                ("Location".to_string(), b"/next".to_vec()),
            ]
        );
    }

    #[test]
    fn a_status_line_sets_the_status_and_adds_no_header() {
        let registry = registry();
        let mut context = TestContext::default();
        send(&mut context, &registry, "HTTP/1.1 404 Not Found");
        let previous = call(
            &registry,
            &mut context,
            "http_response_code",
            vec![Value::Int(410)],
        );
        assert_eq!(previous.expect("code"), Value::Int(404));

        let head = context.state.take::<ResponseHead>().expect("head");
        assert_eq!(head.status, 410);
        assert!(head.headers.is_empty());
    }

    #[test]
    fn a_header_with_a_line_break_is_refused() {
        let registry = registry();
        let mut context = TestContext::default();
        send(&mut context, &registry, "X-A: 1\r\nSet-Cookie: stolen=1");
        assert_eq!(context.diagnostics.messages().len(), 1);
        assert!(context.state.get::<ResponseHead>().is_none());
    }
}
