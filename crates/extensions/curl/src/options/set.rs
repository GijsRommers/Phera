//! `curl_setopt_array($handle, [CURLOPT_URL => $url])` stops at the first failure.

mod connection;
mod strings;

use std::time::Duration;

use phera_extension_api::{
    ArrayKey, Context, LocalFuture, NativeError, NativeResult, Value, args, value::to_bool,
};

use crate::constants::ids::*;
use crate::easy::{Handle, handle};
use crate::error::TransferError;

use super::{Auth, Body, Method, Output, Protocols, Settings};

pub(crate) fn set_option(
    context: &mut dyn Context,
    values: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let handle = handle(&values)?;
        let option = args::int(&values, 1).unwrap_or_default();
        apply(
            context,
            &handle,
            option,
            args::value(&values, 2),
            "curl_setopt",
        )
        .await
    })
}

pub(crate) fn set_array(
    context: &mut dyn Context,
    values: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let handle = handle(&values)?;
        if let Some(array) = args::array(&values, 1) {
            for (key, value) in array.iter() {
                let ArrayKey::Int(option) = key else {
                    return Err(NativeError::value_error(
                        "curl_setopt_array(): Argument #2 ($options) contains an invalid cURL option",
                    ));
                };
                if !to_bool(&apply(context, &handle, option, value, "curl_setopt_array").await?) {
                    return Ok(Value::Bool(false));
                }
            }
        }
        Ok(Value::Bool(true))
    })
}

async fn apply(
    context: &mut dyn Context,
    handle: &Handle,
    option: i64,
    value: &Value,
    function: &str,
) -> NativeResult {
    let value = value.deref().clone();
    let name = crate::constants::option_name(option);
    if name.is_none() && option != CURLINFO_HEADER_OUT {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #2 (${}) {}",
            if function == "curl_setopt" {
                "option"
            } else {
                "options"
            },
            if function == "curl_setopt" {
                "is not a valid cURL option"
            } else {
                "contains an invalid cURL option"
            },
        )));
    }

    // Conversion can call PHP. Publish only this option on the latest handle state.
    let mut text = None;
    let mut list = Vec::new();
    if strings::takes_text(option)
        && !(option == CURLOPT_POSTFIELDS && matches!(value, Value::Array(_)))
    {
        if !matches!(value, Value::Null) || option == CURLOPT_POSTFIELDS {
            let converted = context.stringify(&value).await?;
            if option != CURLOPT_POSTFIELDS && converted.as_bytes().contains(&0) {
                return Err(NativeError::value_error(format!(
                    "{function}(): cURL option must not contain any null bytes"
                )));
            }
            text = Some(converted.as_bytes().to_vec());
        }
    } else if matches!(
        option,
        CURLOPT_HTTPHEADER | CURLOPT_RESOLVE | CURLOPT_CONNECT_TO
    ) {
        let Value::Array(values) = &value else {
            return Err(NativeError::type_error(format!(
                "{function}(): The {} option must have an array value",
                name.unwrap_or_default()
            )));
        };
        for item in values.values() {
            let converted = context.stringify(item).await?;
            if converted.as_bytes().contains(&0) {
                return Err(NativeError::value_error(format!(
                    "{function}(): cURL option must not contain any null bytes"
                )));
            }
            list.push(converted.as_bytes().to_vec());
        }
    }

    if option == CURLOPT_SHARE {
        let share = match &value {
            Value::Object(object) => {
                Some(crate::classes::state::<crate::share::ShareHandle>(object)?)
            }
            Value::Null => None,
            _ => {
                return Err(NativeError::type_error(format!(
                    "{function}(): The CURLOPT_SHARE option must be a CurlShareHandle or CurlSharePersistentHandle"
                )));
            }
        };
        handle.borrow_mut().attach_share(share);
        return Ok(Value::Bool(true));
    }
    let (cookies, jar, files, session, command) = {
        let mut easy = handle.borrow_mut();
        match change(
            context,
            &mut easy.settings,
            option,
            &value,
            text,
            list,
            function,
        )? {
            Change::Applied => {
                easy.error = None;
                if option == CURLOPT_URL {
                    easy.info.url = None;
                }
                if option == CURLOPT_CUSTOMREQUEST {
                    easy.info.method = None;
                }
                return Ok(Value::Bool(true));
            }
            Change::Cookie(command) => (
                easy.cookies(),
                easy.settings.cookie_jar.clone(),
                easy.settings.cookie_files.clone(),
                easy.settings.cookie_session,
                command,
            ),
            Change::Refused(reason) => {
                let message = format!("{}: {reason}", name.unwrap_or("CURLINFO_HEADER_OUT"));
                easy.error = Some(TransferError::new(4, &message));
                context.warning(format!("{function}(): {message}"));
                return Ok(Value::Bool(false));
            }
        }
    };
    crate::cookies::command(context, &cookies, jar.as_deref(), &files, session, &command).await;
    Ok(Value::Bool(true))
}

pub(super) enum Change {
    Applied,
    Cookie(Vec<u8>),
    Refused(&'static str),
}

fn change(
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
                callback(context, function, value)?.map_or(Output::Print, Output::Callback);
        }
        CURLOPT_READFUNCTION => settings.read = callback(context, function, value)?,
        CURLOPT_HEADERFUNCTION => {
            settings.header = callback(context, function, value)?.map(Output::Callback);
        }
        CURLOPT_PROGRESSFUNCTION | CURLOPT_XFERINFOFUNCTION => {
            settings.progress = callback(context, function, value)?
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
        CURLOPT_MAXCONNECTS if number >= 0 => settings.pool_idle = (number as usize).clamp(1, 8),
        CURLOPT_SAFE_UPLOAD if enabled => {}
        CURLOPT_SAFE_UPLOAD => {
            context.warning(format!(
                "{function}(): Disabling safe uploads is no longer supported"
            ));
            return Ok(Change::Refused("safe multipart uploads cannot be disabled"));
        }
        _ => return connection::change(context, settings, option, number, text, list),
    }
    Ok(Change::Applied)
}

fn callback(
    context: &mut dyn Context,
    function: &str,
    value: &Value,
) -> Result<Option<Value>, NativeError> {
    if matches!(value, Value::Null) {
        return Ok(None);
    }
    context.validate_callback(function, 3, "value", value)?;
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
