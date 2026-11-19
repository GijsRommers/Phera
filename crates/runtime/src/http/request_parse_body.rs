//! `[$post, $files] = request_parse_body()` reads form data independently of
//! `$_POST` and `$_FILES`, including for methods such as PUT and PATCH.

use phera_ext_http::{RequestHeaders, UploadedFiles};
use phera_ext_streams::{RequestInput, request_input};
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args,
};

use super::form_limits::FormLimits;
use super::multipart::{BoundaryError, explicit_boundary};
use super::{is_multipart, media_type, query, uploads};

const EXCEPTION: &str = "RequestParseBodyException";
const FUNCTION: &str = "request_parse_body";

pub(crate) fn register_request_parse_body(registry: &mut Registry) {
    const PARAMS: &[Param] = &[Param::new("options", ParamType::Array)
        .nullable()
        .with_default(ConstantValue::Null)];
    registry.function(FunctionEntry::asynchronous(FUNCTION, PARAMS, parse));
    registry.describe_function(FUNCTION, "array", Some("standard"));
}

fn parse(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let limits = FormLimits::from_options(context, args::array(&arguments, 0).map(|a| &**a))?;
        let content_type = content_type(context)?;
        let input = request_input(context.state()).cloned().unwrap_or_default();
        if is_urlencoded(&content_type) {
            let post = parse_urlencoded(context, &input, limits);
            return pair(post, PhpArray::new());
        }
        if is_multipart(&content_type) {
            return parse_multipart(context, &input, &content_type, limits).await;
        }

        // For POST, PHP's default reader also runs for an unknown media type.
        if input.post_read() {
            read_standard(context, &input, limits);
        }
        Err(NativeError::throw_bytes(
            EXCEPTION,
            [
                b"Content-Type \"",
                &content_type[..],
                b"\" is not supported",
            ]
            .concat(),
        ))
    })
}

fn content_type(context: &mut dyn Context) -> Result<Vec<u8>, NativeError> {
    let mut values = context
        .state()
        .get::<RequestHeaders>()
        .into_iter()
        .flat_map(|headers| &headers.0)
        .filter_map(|(name, value)| name.eq_ignore_ascii_case("Content-Type").then_some(value));
    let Some(first) = values.next() else {
        return Err(NativeError::throw(
            EXCEPTION,
            "Request does not provide a content type",
        ));
    };
    let mut value = first.clone();
    for other in values {
        value.extend_from_slice(b", ");
        value.extend_from_slice(other);
    }
    value.truncate(
        value
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(value.len()),
    );
    Ok(value)
}

pub(crate) fn is_urlencoded(content_type: &[u8]) -> bool {
    media_type(content_type).eq_ignore_ascii_case(b"application/x-www-form-urlencoded")
}

fn parse_urlencoded(
    context: &mut dyn Context,
    input: &RequestInput,
    limits: FormLimits,
) -> PhpArray {
    let mut post = PhpArray::new();
    let range = read_standard(context, input, limits);

    let limit = limits.max_input_vars as u64;
    if query::parse_post(&mut post, &input.bytes()[range], limit) {
        context.warning(format!(
            "{FUNCTION}(): Input variables exceeded {limit}. To increase the limit change \
             max_input_vars in php.ini."
        ));
    }
    post
}

fn read_standard(
    context: &mut dyn Context,
    input: &RequestInput,
    limits: FormLimits,
) -> std::ops::Range<usize> {
    match size_issue(input, limits) {
        Some(message) => {
            context.warning(format!("{FUNCTION}(): {message}"));
            input.buffered_urlencoded()
        }
        None => input.read_urlencoded(),
    }
}

async fn parse_multipart(
    context: &mut dyn Context,
    input: &RequestInput,
    content_type: &[u8],
    limits: FormLimits,
) -> NativeResult {
    if let Some(message) = size_issue(input, limits) {
        return Err(NativeError::throw(EXCEPTION, message));
    }

    let boundary = match explicit_boundary(content_type) {
        Ok(boundary) => boundary.to_vec(),
        Err(BoundaryError::Missing) => {
            return Err(NativeError::throw(
                EXCEPTION,
                "Missing boundary in multipart/form-data POST data",
            ));
        }
        Err(BoundaryError::Invalid) => {
            return Err(NativeError::throw(
                EXCEPTION,
                "Invalid boundary in multipart/form-data POST data",
            ));
        }
        Err(BoundaryError::TooLarge) => {
            context.warning("Boundary too large in multipart/form-data POST data".into());
            return pair(PhpArray::new(), PhpArray::new());
        }
    };
    let range = input.read_multipart();
    let reserved = range.clone();
    let bytes = input.bytes().clone();
    let mut form =
        tokio::task::spawn_blocking(move || uploads::receive(&bytes[range], &boundary, limits))
            .await
            .map_err(|error| {
                NativeError::error(format!("Unable to parse request body: {error}"))
            })?;
    let issue = form.issue.take();
    input.finish_multipart(reserved, form.consumed);
    let (post, files, temporary) = form.into_variables();
    context
        .state()
        .get_or_default::<UploadedFiles>()
        .replace_active(temporary);
    match issue {
        Some(issue) => Err(NativeError::throw(EXCEPTION, issue.message())),
        None => pair(post, files),
    }
}

fn size_issue(input: &RequestInput, limits: FormLimits) -> Option<String> {
    let size = input.bytes().len() as u64;
    let limit = limits.post_max_size;
    (limit > 0 && size > limit as u64)
        .then(|| format!("POST Content-Length of {size} bytes exceeds the limit of {limit} bytes"))
}

fn pair(post: PhpArray, files: PhpArray) -> NativeResult {
    let mut result = PhpArray::new();
    result.push(Value::from(post))?;
    result.push(Value::from(files))?;
    Ok(Value::from(result))
}
