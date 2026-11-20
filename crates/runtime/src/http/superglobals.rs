//! Building `$_SERVER`, `$_GET`, `$_POST`, `$_COOKIE` and `$_REQUEST` from a
//! request.
//!
//! A header the client sends more than once reaches PHP once, its values
//! joined with ", " in the order they arrived:
//!
//! ```text
//! X-Tag: one
//! x-tag: two              $_SERVER['HTTP_X_TAG'] === 'one, two'
//! ```
//!
//! That joined value is what PHP itself looks at afterwards, so two `Cookie`
//! headers give one cookie string, and two `Content-Type` headers describe a
//! form only when the form type comes first.

use std::path::Path;

use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempPath;

use phera_value::{ArrayKey, PhpArray, Value};
use phera_vm::Vm;

use super::query::{Duplicates, parse_pairs};
use super::{Form, HttpRequest, is_urlencoded};

/// Defines the superglobals for one request before its script starts.
pub(crate) fn define_superglobals(
    vm: &mut Vm,
    request: &HttpRequest,
    script: &Path,
    document_root: &Path,
    environment: PhpArray,
    form: Option<Form>,
) -> Vec<TempPath> {
    let mut get = PhpArray::new();
    parse_pairs(
        &mut get,
        request.query.as_bytes(),
        b'&',
        Duplicates::Replace,
    );

    let headers = join_repeated_headers(request);

    let mut post = PhpArray::new();
    // PHP fills `$_POST` for the method `POST` only, spelled exactly so. A
    // form body sent with `PUT`, `GET` or `post` stays in the raw request
    // body.
    let mut files = PhpArray::new();
    let mut temporary_files = Vec::new();
    if let Some(form) = form {
        // A form with files: already read, the files already on disk.
        (post, files, temporary_files) = form.into_variables();
    } else if request.method == "POST"
        && header(&headers, "content-type").is_some_and(is_urlencoded)
    {
        parse_pairs(&mut post, &request.body, b'&', Duplicates::Replace);
    }

    let mut cookie = PhpArray::new();
    if let Some(cookies) = header(&headers, "cookie") {
        parse_pairs(&mut cookie, cookies, b';', Duplicates::KeepFirst);
    }

    // PHP's default `request_order` is "GP": POST values override GET ones,
    // and cookies are left out.
    let mut merged = get.clone();
    for (key, value) in post.iter() {
        merged.insert(key.clone(), value.clone());
    }

    vm.set_global(
        "_SERVER",
        Value::from(server(
            request,
            &headers,
            script,
            document_root,
            environment,
        )),
    );
    vm.set_global("_GET", Value::from(get));
    vm.set_global("_POST", Value::from(post));
    vm.set_global("_COOKIE", Value::from(cookie));
    vm.set_global("_REQUEST", Value::from(merged));
    vm.set_global("_FILES", Value::from(files));
    temporary_files
}

/// One request header as PHP sees it.
struct JoinedHeader<'a> {
    /// Header names do not distinguish case.
    name: &'a str,
    /// Every value sent under this name, joined with ", ".
    value: std::borrow::Cow<'a, [u8]>,
}

/// Collects the headers by name, in the order each name first appeared.
fn join_repeated_headers(request: &HttpRequest) -> Vec<JoinedHeader<'_>> {
    let mut joined: Vec<JoinedHeader<'_>> = Vec::with_capacity(request.headers.len());
    for (name, value) in &request.headers {
        let earlier = joined
            .iter_mut()
            .find(|header| header.name.eq_ignore_ascii_case(name));
        match earlier {
            Some(header) => {
                let joined = header.value.to_mut();
                joined.extend_from_slice(b", ");
                joined.extend_from_slice(value);
            }
            None => joined.push(JoinedHeader {
                name,
                value: std::borrow::Cow::Borrowed(value),
            }),
        }
    }
    joined
}

/// The joined value of the header with this lower-case name.
fn header<'a>(headers: &'a [JoinedHeader<'_>], name: &str) -> Option<&'a [u8]> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.as_ref())
}

fn server(
    request: &HttpRequest,
    headers: &[JoinedHeader<'_>],
    script: &Path,
    document_root: &Path,
    mut server: PhpArray,
) -> PhpArray {
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
    if request.https {
        // The key is absent for plain HTTP, which is what scripts test.
        set("HTTPS", Value::from("on"));
    }
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

    for header in headers {
        // CGI's convention: `Accept-Language` becomes `HTTP_ACCEPT_LANGUAGE`.
        // The two headers that describe the body are also set without the
        // prefix, as PHP's own server does.
        let upper = header.name.to_ascii_uppercase().replace('-', "_");
        if matches!(upper.as_str(), "CONTENT_TYPE" | "CONTENT_LENGTH") {
            set(&upper, Value::from(header.value.as_ref()));
        }
        set(&format!("HTTP_{upper}"), Value::from(header.value.as_ref()));
    }
    server
}

/// The superglobals of a boot run (see `crate::boot`): no request. `$_SERVER`
/// holds the environment and what PHP's command line sets without a
/// request: the script's paths (`SCRIPT_FILENAME`, `SCRIPT_NAME`,
/// `PHP_SELF`), `DOCUMENT_ROOT`, `SERVER_SOFTWARE` and the start time
/// (`REQUEST_TIME`, `REQUEST_TIME_FLOAT`), which each request replaces with
/// its own. Nothing a client sends is there.
pub(crate) fn define_boot_superglobals(
    vm: &mut Vm,
    script: &Path,
    document_root: &Path,
    mut server: PhpArray,
) {
    let script_name = script
        .strip_prefix(document_root)
        .map(|relative| format!("/{}", relative.to_string_lossy()))
        .unwrap_or_else(|_| script.to_string_lossy().into_owned());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let mut set = |name: &str, value: Value| {
        server.insert(ArrayKey::str(name), value);
    };
    set("SERVER_SOFTWARE", Value::from("Phera"));
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
    vm.set_global("_SERVER", Value::from(server));
    for name in ["_GET", "_POST", "_COOKIE", "_REQUEST", "_FILES"] {
        vm.set_global(name, Value::from(PhpArray::new()));
    }
}
