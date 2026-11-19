//! `curl_setopt($handle, CURLOPT_POST, true)` changes the configured method.

use std::time::Duration;

use phera_extension_api::{Context, NativeError, Value, value::to_bool};

use crate::{
    constants::ids::*,
    options::{Auth, Body, Method, Output, Protocols, Settings},
};

use super::{Change, connection};

pub(super) fn change(
    context: &mut dyn Context,
    settings: &mut Settings,
    option: i64,
    value: &Value,
    text: Option<Vec<u8>>,
    list: Vec<Vec<u8>>,
    function: &str,
) -> Result<Change, NativeError> {
    let enabled = to_bool(value);
    let number = if option < 10_000
        || matches!(option, CURLOPT_MAXFILESIZE_LARGE | CURLOPT_INFILESIZE_LARGE)
    {
        phera_extension_api::value::to_int(value, context.diagnostics())?
    } else {
        0
    };
    match option {
        CURLOPT_HTTPHEADER => settings.headers = list,
        CURLOPT_POSTFIELDS => {
            settings.body = Some(match value {
                Value::Array(array) => Body::Multipart(array.clone()),
                _ => Body::Text(text.unwrap_or_default()),
            });
            settings.method = Method::Post;
        }
        CURLOPT_HTTPGET if enabled => settings.method = Method::Get,
        CURLOPT_POST => settings.method = if enabled { Method::Post } else { Method::Get },
        CURLOPT_UPLOAD | CURLOPT_PUT => {
            settings.method = if enabled { Method::Put } else { Method::Get }
        }
        CURLOPT_NOBODY => {
            if enabled {
                settings.method = Method::Head;
            } else if settings.method == Method::Head {
                settings.method = Method::Get;
            }
        }
        CURLOPT_HTTPGET => {}
        CURLOPT_INFILE => settings.input = stream(value, function, "CURLOPT_INFILE")?,
        CURLOPT_INFILESIZE | CURLOPT_INFILESIZE_LARGE => {
            settings.input_size = u64::try_from(number).ok();
        }
        CURLOPT_RETURNTRANSFER => {
            settings.output = if enabled {
                Output::Return
            } else {
                Output::Print
            };
        }
        CURLOPT_HEADER => settings.include_header = enabled,
        CURLOPT_WRITEFUNCTION => {
            settings.output =
                callback(context, function, option, value)?.map_or(Output::Print, Output::Callback);
        }
        CURLOPT_READFUNCTION => settings.read = callback(context, function, option, value)?,
        CURLOPT_HEADERFUNCTION => {
            settings.header = callback(context, function, option, value)?.map(Output::Callback);
        }
        CURLOPT_PROGRESSFUNCTION | CURLOPT_XFERINFOFUNCTION => {
            settings.progress = callback(context, function, option, value)?
                .map(|callback| (callback, option == CURLOPT_XFERINFOFUNCTION));
        }
        CURLOPT_FILE => {
            settings.output =
                stream(value, function, "CURLOPT_FILE")?.map_or(Output::Print, Output::Stream);
        }
        CURLOPT_WRITEHEADER => {
            settings.header = stream(value, function, "CURLOPT_WRITEHEADER")?.map(Output::Stream);
        }
        CURLOPT_STDERR => settings.stderr = stream(value, function, "CURLOPT_STDERR")?,
        CURLOPT_VERBOSE => settings.verbose = enabled,
        CURLINFO_HEADER_OUT => settings.header_out = enabled,
        CURLOPT_NOPROGRESS => settings.no_progress = enabled,
        CURLOPT_FOLLOWLOCATION => settings.follow = enabled,
        CURLOPT_MAXREDIRS => settings.max_redirects = number,
        CURLOPT_POSTREDIR => settings.post_redirect = (number & 7) as u8,
        CURLOPT_AUTOREFERER => settings.auto_referer = enabled,
        CURLOPT_UNRESTRICTED_AUTH => settings.unrestricted_auth = enabled,
        CURLOPT_TIMEOUT | CURLOPT_TIMEOUT_MS => {
            settings.timeout = duration(number, option == CURLOPT_TIMEOUT_MS);
        }
        CURLOPT_CONNECTTIMEOUT | CURLOPT_CONNECTTIMEOUT_MS => {
            settings.connect_timeout = duration(number, option == CURLOPT_CONNECTTIMEOUT_MS)
                .or(Some(Duration::from_secs(300)));
        }
        CURLOPT_LOW_SPEED_LIMIT => settings.low_speed_limit = number.max(0) as u64,
        CURLOPT_LOW_SPEED_TIME => {
            settings.low_speed_time = Duration::from_secs(number.max(0) as u64)
        }
        CURLOPT_HTTP_CONTENT_DECODING => settings.decode = enabled,
        CURLOPT_COOKIESESSION => settings.cookie_session = enabled,
        CURLOPT_HTTPAUTH => match number {
            0 => settings.auth = Auth::None,
            1 => settings.auth = Auth::Basic,
            64 => settings.auth = Auth::Bearer,
            65 | CURLAUTH_ANY => settings.auth = Auth::BasicOrBearer,
            _ => {
                return Ok(Change::Refused(
                    "only Basic and Bearer authentication are available",
                ));
            }
        },
        CURLOPT_FAILONERROR => settings.fail_on_error = enabled,
        CURLOPT_FILETIME => settings.file_time = enabled,
        CURLOPT_MAXFILESIZE | CURLOPT_MAXFILESIZE_LARGE => {
            settings.max_file_size = number.max(0) as u64;
        }
        CURLOPT_BUFFERSIZE => settings.buffer_size = number.clamp(1024, 512 * 1024) as usize,
        CURLOPT_FRESH_CONNECT => settings.fresh = enabled,
        CURLOPT_FORBID_REUSE => settings.forbid_reuse = enabled,
        CURLOPT_PRIVATE => settings.private = Some(value.clone()),
        CURLOPT_PROTOCOLS | CURLOPT_REDIR_PROTOCOLS => {
            let protocols = Protocols {
                http: number & 1 != 0,
                https: number & 2 != 0,
            };
            if option == CURLOPT_PROTOCOLS {
                settings.protocols = protocols;
            } else {
                settings.redirect_protocols = protocols;
            }
        }
        CURLOPT_SSL_VERIFYPEER if enabled => {}
        CURLOPT_SSL_VERIFYHOST if number == 1 || number == 2 => {}
        CURLOPT_NOSIGNAL | CURLOPT_BINARYTRANSFER => {}
        CURLOPT_DNS_USE_GLOBAL_CACHE | CURLOPT_CERTINFO if !enabled => {}
        CURLOPT_HEADEROPT if number == CURLHEADER_SEPARATE => {}
        CURLOPT_HTTP_TRANSFER_DECODING if enabled => {}
        CURLOPT_SUPPRESS_CONNECT_HEADERS if enabled => {}
        CURLOPT_PIPEWAIT if enabled => {}
        CURLOPT_PREREQFUNCTION if matches!(value, Value::Null) => {}
        CURLOPT_MAXCONNECTS if number >= 0 => settings.pool_idle = (number as usize).clamp(1, 8),
        CURLOPT_SAFE_UPLOAD if enabled => {}
        CURLOPT_SAFE_UPLOAD => {
            return Err(NativeError::value_error(format!(
                "{function}(): Disabling safe uploads is no longer supported"
            )));
        }
        _ => return connection::change(context, settings, option, number, text, list),
    }
    Ok(Change::Applied)
}

fn callback(
    context: &mut dyn Context,
    function: &str,
    option: i64,
    value: &Value,
) -> Result<Option<Value>, NativeError> {
    if matches!(value, Value::Null) {
        return Ok(None);
    }
    let (position, parameter) = if function == "curl_setopt_array" {
        (2, "options")
    } else {
        (3, "value")
    };
    context
        .validate_callback(function, position, parameter, value)
        .map_err(|error| {
            let NativeError::Throw {
                class: "TypeError",
                message,
                ..
            } = &error
            else {
                return error;
            };
            let option = crate::constants::option_name(option).unwrap_or_default();
            NativeError::type_error(message.replacen(
                "must be a valid callback",
                &format!("must be a valid callback for option {option}"),
                1,
            ))
        })?;
    Ok(Some(value.clone()))
}

fn stream(
    value: &Value,
    function: &str,
    option: &str,
) -> Result<Option<std::rc::Rc<phera_extension_api::value::Resource>>, NativeError> {
    match value {
        Value::Resource(stream) if stream.kind == "stream" && !stream.is_closed() => {
            Ok(Some(stream.clone()))
        }
        Value::Null => Ok(None),
        _ => Err(NativeError::type_error(format!(
            "{function}(): The {option} option must have a stream resource value"
        ))),
    }
}

fn duration(number: i64, milliseconds: bool) -> Option<Duration> {
    (number > 0).then(|| {
        if milliseconds {
            Duration::from_millis(number as u64)
        } else {
            Duration::from_secs(number as u64)
        }
    })
}
