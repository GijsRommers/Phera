//! `fopen('http://localhost/results', 'r')` exposes HTTP response bytes and headers.

mod exchange;

use std::{io, io::Cursor};

use base64::{Engine, engine::general_purpose::STANDARD};
use http_body_util::{BodyExt, Limited};
use hyper::{Uri, header};
use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args, value::to_bool,
};
use url::Url;

use self::exchange::exchange;
use super::{default_timeout, http_headers};
use crate::context_options;
use crate::stream::{Backing, Stream, Wrapper};

const MAX_BODY: usize = 16 * 1024 * 1024;

pub(super) fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        Param::new("url", ParamType::String),
        Param::new("associative", ParamType::Bool).with_default(ConstantValue::Bool(false)),
        context_options::OPTIONAL_CONTEXT,
    ];
    http_headers::register(registry);
    registry.function(FunctionEntry::asynchronous(
        "get_headers",
        PARAMS,
        get_headers,
    ));
}

pub(crate) fn is_url(path: &[u8]) -> bool {
    path.get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"http://"))
        || path
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"https://"))
}

pub(crate) async fn fopen(
    context: &mut dyn Context,
    function: &str,
    name: &[u8],
    mode: &[u8],
    supplied: &Value,
    use_default: bool,
) -> NativeResult {
    let path = String::from_utf8_lossy(name);
    if !matches!(mode, b"r" | b"rb" | b"rt") {
        return failed(
            context,
            function,
            &path,
            "HTTP wrapper does not support writeable connections",
        );
    }
    let attached = if supplied.is_null() && use_default {
        context_options::default_for_wrapper(context)
    } else {
        supplied.clone()
    };
    let Some((body, lines)) = open(context, function, &path, &attached, false).await? else {
        return Ok(Value::Bool(false));
    };
    let mut stream = Stream::new(
        Backing::Memory(Cursor::new(body)),
        Wrapper::Http,
        mode,
        name,
    );
    stream.seekable = false;
    stream.data_header = Some(lines.join(&b'\n'));
    let stream = crate::request::new_resource(context.state(), "stream", stream);
    context_options::attach(context, &stream, &attached);
    Ok(stream)
}

fn get_headers(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "get_headers";
        let name = args::bytes(&arguments, 0);
        crate::errors::check_path(function, "#1 ($url)", name)?;
        crate::errors::nonempty_path(name)?;
        let supplied = args::value(&arguments, 2);
        context_options::validate(supplied, function, 3)?;
        if !is_url(name) {
            if let Some(scheme) = crate::user_wrappers::protocol(name)
                && !scheme.eq_ignore_ascii_case(b"file")
                && !scheme.eq_ignore_ascii_case(b"php")
            {
                crate::errors::wrapper_warning(context, function, scheme);
            }
            context.warning("get_headers(): This function may only be used against URLs".into());
            return Ok(Value::Bool(false));
        }
        let attached = if supplied.is_null() {
            context_options::default_for_wrapper(context)
        } else {
            supplied.clone()
        };
        let path = String::from_utf8_lossy(name);
        let Some((_, lines)) = open(context, function, &path, &attached, true).await? else {
            return Ok(Value::Bool(false));
        };
        let associative = args::bool(&arguments, 1).unwrap_or(false);
        Ok(Value::from(http_headers::values(&lines, associative)?))
    })
}

type Reply = (Vec<u8>, Vec<Vec<u8>>);

async fn open(
    context: &mut dyn Context,
    function: &str,
    path: &str,
    attached: &Value,
    headers_only: bool,
) -> Result<Option<Reply>, NativeError> {
    if !crate::user_file::enabled(context, b"allow_url_fopen", true) {
        let scheme = if path.as_bytes()[..5].eq_ignore_ascii_case(b"https") {
            "https"
        } else {
            "http"
        };
        context.warning(format!(
            "{function}(): {scheme}:// wrapper is disabled in the server configuration by allow_url_fopen=0"
        ));
        failed(
            context,
            function,
            path,
            "no suitable wrapper could be found",
        )?;
        return Ok(None);
    }
    http_headers::clear(context);
    let options = context_options::for_wrapper(attached, "http");
    let ssl = context_options::for_wrapper(attached, "ssl");
    let timeout = match option(&options, "timeout") {
        Some(Value::Float(seconds)) => std::time::Duration::try_from_secs_f64(*seconds).ok(),
        Some(Value::Int(seconds)) => u64::try_from(*seconds)
            .ok()
            .map(std::time::Duration::from_secs),
        _ => default_timeout(context),
    };
    let mut lines = Vec::new();
    let request = fetch(path, &options, &ssl, headers_only, &mut lines);
    let response = match timeout {
        Some(timeout) => tokio::time::timeout(timeout, request)
            .await
            .unwrap_or_else(|_| Err(io::ErrorKind::TimedOut.into())),
        None => request.await,
    };
    if !lines.is_empty() {
        http_headers::save(context, &lines)?;
    }
    match response {
        Ok(body) => Ok(Some((body, lines))),
        Err(error) => {
            failed(context, function, path, &error.to_string())?;
            Ok(None)
        }
    }
}

async fn fetch(
    path: &str,
    options: &PhpArray,
    ssl: &PhpArray,
    headers_only: bool,
    lines: &mut Vec<Vec<u8>>,
) -> io::Result<Vec<u8>> {
    if option(options, "proxy").is_some() {
        return Err(invalid("HTTP proxy context options are not supported"));
    }
    let mut url = Url::parse(path).map_err(invalid)?;
    url.set_fragment(None);
    let origin = url.origin();
    let mut use_get = false;
    let mut left = match option(options, "max_redirects") {
        Some(Value::Int(count)) => (*count).clamp(1, 100),
        _ => 20,
    };
    loop {
        let uri: Uri = url.as_str().parse().map_err(invalid)?;
        let same_origin = url.origin() == origin;
        let authorization = if same_origin {
            url_authorization(&url)?
        } else {
            None
        };
        let (reply, wire, _task) =
            exchange(uri, options, ssl, same_origin, use_get, authorization).await?;
        let status = reply.status();
        let status_line = wire.first().cloned().unwrap_or_default();
        let location = reply
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok());
        let redirect = location.filter(|_| {
            matches!(status.as_u16(), 300 | 301 | 302 | 303 | 307 | 308)
                && option(options, "follow_location").is_none_or(to_bool)
        });
        lines.extend(wire.into_iter().filter(|line| {
            headers_only
                || !http_headers::field(line).is_some_and(|(name, value)| {
                    name.eq_ignore_ascii_case(b"Transfer-Encoding")
                        && value.eq_ignore_ascii_case(b"chunked")
                })
        }));
        if let Some(redirect) = redirect {
            left -= 1;
            if left == 0 && !headers_only {
                return Err(invalid("Redirection limit reached, aborting"));
            }
            if left > 0 {
                let mut next = url.join(redirect).map_err(invalid)?;
                if !matches!(next.scheme(), "http" | "https") {
                    return Err(invalid("Redirect requires an HTTP or HTTPS URL"));
                }
                next.set_fragment(None);
                url = next;
                use_get |= !matches!(status.as_u16(), 307 | 308);
                continue;
            }
        }
        if headers_only {
            return Ok(Vec::new());
        }
        if (status.is_client_error() || status.is_server_error())
            && !option(options, "ignore_errors").is_some_and(to_bool)
        {
            return Err(invalid(format!(
                "HTTP request failed! {}\r\n",
                String::from_utf8_lossy(&status_line)
            )));
        }
        let bytes = Limited::new(reply.into_body(), MAX_BODY)
            .collect()
            .await
            .map_err(invalid)?;
        return Ok(bytes.to_bytes().to_vec());
    }
}

/// Userinfo is decoded as bytes before adding the Basic separator.
fn url_authorization(url: &Url) -> io::Result<Option<header::HeaderValue>> {
    let password = url.password();
    if url.username().is_empty() && password.is_none() {
        return Ok(None);
    }
    let mut credentials: Vec<_> = percent_encoding::percent_decode_str(url.username()).collect();
    credentials.push(b':');
    credentials.extend(percent_encoding::percent_decode_str(password.unwrap_or("")));
    let mut value =
        header::HeaderValue::from_str(&format!("Basic {}", STANDARD.encode(credentials)))
            .map_err(invalid)?;
    value.set_sensitive(true);
    Ok(Some(value))
}

fn option<'a>(options: &'a PhpArray, key: &str) -> Option<&'a Value> {
    options.get(&ArrayKey::str(key))
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error.to_string())
}

fn failed(context: &mut dyn Context, function: &str, path: &str, detail: &str) -> NativeResult {
    context.warning(format!(
        "{function}({path}): Failed to open stream: {detail}"
    ));
    Ok(Value::Bool(false))
}
