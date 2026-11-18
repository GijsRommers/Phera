//! Request variables exposed to PHP. Two `X-Tag` fields reach
//! `$_SERVER['HTTP_X_TAG']` as a single comma-separated string.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use phera_value::{ArrayKey, PhpArray, Value};
use phera_vm::Vm;

use super::HttpRequest;
use super::query::{Duplicates, parse_pairs};

/// Defines the superglobals for one request before its script starts.
pub(crate) fn define_superglobals(
    vm: &mut Vm,
    request: &HttpRequest,
    script: &Path,
    document_root: &Path,
) {
    let mut get = PhpArray::new();
    parse_pairs(
        &mut get,
        request.query.as_bytes(),
        b'&',
        Duplicates::Replace,
    );

    let headers = joined_headers(request);
    let mut post = PhpArray::new();
    // PHP fills `$_POST` for the POST method only. A form body sent with PUT
    // or GET stays in the raw request body.
    if request.method == "POST" && is_form(header(&headers, "content-type")) {
        parse_pairs(&mut post, &request.body, b'&', Duplicates::Replace);
    }

    let mut cookie = PhpArray::new();
    if let Some(header) = header(&headers, "cookie") {
        parse_pairs(&mut cookie, header, b';', Duplicates::KeepFirst);
    }

    // PHP's default `request_order` is "GP": POST values override GET ones,
    // and cookies are left out.
    let mut merged = get.clone();
    for (key, value) in post.iter() {
        merged.insert(key.clone(), value.clone());
    }

    vm.set_global(
        "_SERVER",
        Value::from(server(request, headers, script, document_root)),
    );
    vm.set_global("_GET", Value::from(get));
    vm.set_global("_POST", Value::from(post));
    vm.set_global("_COOKIE", Value::from(cookie));
    vm.set_global("_REQUEST", Value::from(merged));
    vm.set_global("_FILES", Value::from(PhpArray::new()));
}

/// Whether the body is an HTML form in the default encoding. File uploads
/// (`multipart/form-data`) are not parsed yet.
fn is_form(content_type: Option<&Cow<'_, [u8]>>) -> bool {
    // A form has one media type. A joined Content-Type is ambiguous even
    // when the first field has a semicolon before the comma.
    content_type.is_some_and(|value| {
        let Cow::Borrowed(value) = value else {
            return false;
        };
        let media_type = value.split(|&byte| byte == b';').next().unwrap_or_default();
        media_type
            .trim_ascii()
            .eq_ignore_ascii_case(b"application/x-www-form-urlencoded")
    })
}

/// Group HTTP field names before applying CGI's dash-to-underscore mapping.
type Headers<'request> = Vec<(String, Cow<'request, [u8]>)>;

fn joined_headers(request: &HttpRequest) -> Headers<'_> {
    let mut positions = HashMap::<String, usize>::new();
    let mut headers = Headers::new();
    for (name, value) in &request.headers {
        let name = name.to_ascii_lowercase();
        if let Some(&position) = positions.get(&name) {
            let joined = headers[position].1.to_mut();
            joined.extend_from_slice(b", ");
            joined.extend_from_slice(value);
        } else {
            positions.insert(name.clone(), headers.len());
            headers.push((name, Cow::Borrowed(value)));
        }
    }
    headers
}

fn header<'headers, 'request>(
    headers: &'headers Headers<'request>,
    name: &str,
) -> Option<&'headers Cow<'request, [u8]>> {
    headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

fn server(
    request: &HttpRequest,
    headers: Headers<'_>,
    script: &Path,
    document_root: &Path,
) -> PhpArray {
    let mut server = PhpArray::new();
    let mut set = |name: &str, value: Value| {
        server.insert(ArrayKey::str(name), value);
    };

    let script_name = script
        .strip_prefix(document_root)
        .map(|relative| format!("/{}", relative.to_string_lossy()))
        .unwrap_or_else(|_| request.path.clone());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    set("SERVER_SOFTWARE", Value::from("Phera"));
    set("SERVER_PROTOCOL", Value::from(request.protocol.as_str()));
    set(
        "SERVER_ADDR",
        Value::from(request.local_addr.ip().to_string()),
    );
    set(
        "SERVER_PORT",
        Value::from(request.local_addr.port().to_string()),
    );
    set(
        "REMOTE_ADDR",
        Value::from(request.remote_addr.ip().to_string()),
    );
    set(
        "REMOTE_PORT",
        Value::from(request.remote_addr.port().to_string()),
    );
    set("REQUEST_METHOD", Value::from(request.method.as_str()));
    set("REQUEST_URI", Value::from(request.target.as_str()));
    set("QUERY_STRING", Value::from(request.query.as_str()));
    set(
        "DOCUMENT_ROOT",
        Value::from(document_root.to_string_lossy().into_owned()),
    );
    set(
        "SCRIPT_FILENAME",
        Value::from(script.to_string_lossy().into_owned()),
    );
    set("SCRIPT_NAME", Value::from(script_name.as_str()));
    set("PHP_SELF", Value::from(script_name.as_str()));
    set("REQUEST_TIME", Value::Int(now.as_secs() as i64));
    set("REQUEST_TIME_FLOAT", Value::Float(now.as_secs_f64()));

    // The host without its port, as the client named it.
    let host = request.header("Host").unwrap_or_default();
    let name = host.split(|&byte| byte == b':').next().unwrap_or_default();
    set("SERVER_NAME", Value::from(name));

    for (header, value) in headers {
        // CGI's convention: `Accept-Language` becomes `HTTP_ACCEPT_LANGUAGE`,
        // except for the two headers that describe the body.
        let upper = header.to_ascii_uppercase().replace('-', "_");
        let key = match upper.as_str() {
            "CONTENT_TYPE" | "CONTENT_LENGTH" => upper,
            _ => format!("HTTP_{upper}"),
        };
        set(&key, Value::from(value.as_ref()));
    }
    server
}
