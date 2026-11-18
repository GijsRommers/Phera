//! The response side of a web request: `header()`, `http_response_code()`
//! and their relatives.
//!
//! PHP keeps the status code and the header list in global state that the
//! web server reads when the script ends. Here that state is a
//! [`ResponseHead`] stored in the request's [`RequestState`]: these functions
//! write it, and the runtime takes it out after the script has finished.
//!
//! [`RequestState`]: phera_extension_api::RequestState

mod head;
mod uploads;

use phera_extension_api::{
    Context, Extension, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};

pub use head::ResponseHead;
pub use uploads::UploadedFiles;

pub struct Http;

const HEADER_PARAMS: &[Param] = &[
    Param::new("header", ParamType::String),
    Param::new("replace", ParamType::Bool).optional(),
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

    // A header may not contain a line break: that is how one request could
    // smuggle a second header, or a body, into the response.
    if line.iter().any(|byte| matches!(byte, b'\r' | b'\n' | 0)) {
        context.warning(
            "Header may not contain more than a single header, new line detected".to_string(),
        );
        return Ok(Value::Null);
    }

    let head = context.state().get_or_default::<ResponseHead>();
    head.apply(line, replace);
    if let Ok(status @ 100..=599) = u16::try_from(status) {
        head.status = status;
    }
    Ok(Value::Null)
}

fn header_remove(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let head = context.state().get_or_default::<ResponseHead>();
    match args::string(arguments, 0) {
        Some(name) => head.remove(&name.to_string_lossy()),
        None => head.headers.clear(),
    }
    Ok(Value::Null)
}

fn headers_list(context: &mut dyn Context, _arguments: &[Value]) -> NativeResult {
    let head = context.state().get_or_default::<ResponseHead>();
    let mut list = PhpArray::new();
    for (name, value) in &head.headers {
        let mut line = name.clone().into_bytes();
        line.extend_from_slice(b": ");
        line.extend_from_slice(value);
        list.push(Value::from(line))?;
    }
    Ok(Value::from(list))
}

/// Phera builds the whole response before sending anything, so headers can
/// still change until the script ends.
fn headers_sent(_context: &mut dyn Context, _arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(false))
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
