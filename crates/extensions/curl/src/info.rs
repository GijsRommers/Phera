//! `curl_getinfo($handle, CURLINFO_RESPONSE_CODE)` reports the latest response.

mod fields;

use std::net::SocketAddr;
use std::time::Duration;

use phera_extension_api::{ArrayKey, Context, NativeResult, PhpArray, Value, args};

use crate::constants::ids::*;
use crate::easy::{Easy, handle};

#[derive(Clone, Default)]
pub(crate) struct Timing {
    pub(crate) queue: Duration,
    pub(crate) lookup: Duration,
    pub(crate) connect: Duration,
    pub(crate) tls: Duration,
    pub(crate) pretransfer: Duration,
    pub(crate) first_byte: Duration,
    pub(crate) redirect: Duration,
    pub(crate) sent: Duration,
    pub(crate) total: Duration,
}

#[derive(Clone, Default)]
pub(crate) struct Info {
    pub(crate) url: Option<String>,
    pub(crate) content_type: Option<Vec<u8>>,
    pub(crate) status: i64,
    pub(crate) header_size: i64,
    pub(crate) request_size: i64,
    pub(crate) filetime: Option<i64>,
    pub(crate) ssl_verify_result: i64,
    pub(crate) redirect_count: i64,
    pub(crate) timing: Timing,
    pub(crate) uploaded: i64,
    pub(crate) downloaded: i64,
    pub(crate) download_length: Option<i64>,
    pub(crate) upload_length: Option<i64>,
    pub(crate) redirect_url: Option<String>,
    pub(crate) peer: Option<SocketAddr>,
    pub(crate) local: Option<SocketAddr>,
    pub(crate) http_version: i64,
    pub(crate) scheme: Option<String>,
    pub(crate) request_header: Option<Vec<u8>>,
    pub(crate) method: Option<String>,
    pub(crate) connection_id: Option<i64>,
    pub(crate) new_connections: i64,
    pub(crate) proxy: bool,
    pub(crate) proxy_connect_code: i64,
    pub(crate) auth: i64,
    pub(crate) proxy_auth: i64,
    pub(crate) auth_available: i64,
    pub(crate) proxy_auth_available: i64,
    pub(crate) referer: Option<Vec<u8>>,
    pub(crate) retry_after: i64,
}

pub(crate) fn get(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let handle = handle(values)?;
    let easy = handle.borrow();
    if let Some(option) = args::int(values, 1) {
        return field(&easy, option);
    }
    let mut result = PhpArray::new();
    for &(name, option) in fields::ARRAY_FIELDS {
        let mut value = field(&easy, option)?;
        if matches!(value, Value::Bool(false)) {
            value = if option == CURLINFO_CONTENT_TYPE {
                Value::Null
            } else if option == CURLINFO_HEADER_OUT {
                continue;
            } else {
                Value::from("")
            };
        }
        result.insert(ArrayKey::str(name), value);
    }
    Ok(Value::from(result))
}

fn micros(time: Duration) -> i64 {
    i64::try_from(time.as_micros()).unwrap_or(i64::MAX)
}

fn speed(bytes: i64, time: Duration) -> i64 {
    if time.is_zero() {
        0
    } else {
        (bytes as f64 / time.as_secs_f64()) as i64
    }
}

fn field(easy: &Easy, option: i64) -> NativeResult {
    let info = &easy.info;
    let time = &info.timing;
    let url = |s: &Option<String>| s.clone().map_or(Value::Bool(false), Value::from);
    let bytes = |s: &Option<Vec<u8>>| s.clone().map_or(Value::Bool(false), Value::from);
    let ip = |address: Option<SocketAddr>| {
        Value::from(address.map_or_else(String::new, |address| address.ip().to_string()))
    };
    let port = |address: Option<SocketAddr>| {
        Value::Int(address.map_or(-1, |address| i64::from(address.port())))
    };
    Ok(match option {
        CURLINFO_EFFECTIVE_URL => match (easy.settings.url(), &info.url) {
            (_, Some(value)) => Value::from(value.clone()),
            (None, None) => Value::from(""),
            (Some(_), None) => easy
                .settings
                .normalized_url()
                .map_or(Value::Bool(false), |url| Value::from(url.to_string())),
        },
        CURLINFO_CONTENT_TYPE => bytes(&info.content_type),
        CURLINFO_RESPONSE_CODE => Value::Int(info.status),
        CURLINFO_HEADER_SIZE => Value::Int(info.header_size),
        CURLINFO_REQUEST_SIZE => Value::Int(info.request_size),
        CURLINFO_FILETIME | CURLINFO_FILETIME_T => Value::Int(info.filetime.unwrap_or(-1)),
        CURLINFO_SSL_VERIFYRESULT => Value::Int(info.ssl_verify_result),
        CURLINFO_REDIRECT_COUNT => Value::Int(info.redirect_count),
        CURLINFO_TOTAL_TIME => Value::Float(time.total.as_secs_f64()),
        CURLINFO_NAMELOOKUP_TIME => Value::Float(time.lookup.as_secs_f64()),
        CURLINFO_CONNECT_TIME => Value::Float(time.connect.as_secs_f64()),
        CURLINFO_APPCONNECT_TIME => Value::Float(time.tls.as_secs_f64()),
        CURLINFO_PRETRANSFER_TIME => Value::Float(time.pretransfer.as_secs_f64()),
        CURLINFO_STARTTRANSFER_TIME => Value::Float(time.first_byte.as_secs_f64()),
        CURLINFO_REDIRECT_TIME => Value::Float(time.redirect.as_secs_f64()),
        CURLINFO_TOTAL_TIME_T => Value::Int(micros(time.total)),
        CURLINFO_NAMELOOKUP_TIME_T => Value::Int(micros(time.lookup)),
        CURLINFO_CONNECT_TIME_T => Value::Int(micros(time.connect)),
        CURLINFO_APPCONNECT_TIME_T => Value::Int(micros(time.tls)),
        CURLINFO_PRETRANSFER_TIME_T => Value::Int(micros(time.pretransfer)),
        CURLINFO_STARTTRANSFER_TIME_T => Value::Int(micros(time.first_byte)),
        CURLINFO_REDIRECT_TIME_T => Value::Int(micros(time.redirect)),
        CURLINFO_POSTTRANSFER_TIME_T => Value::Int(micros(time.sent)),
        CURLINFO_QUEUE_TIME_T => Value::Int(micros(time.queue)),
        CURLINFO_SIZE_UPLOAD => Value::Float(info.uploaded as f64),
        CURLINFO_SIZE_DOWNLOAD => Value::Float(info.downloaded as f64),
        CURLINFO_SPEED_DOWNLOAD => Value::Float(speed(info.downloaded, time.total) as f64),
        CURLINFO_SPEED_UPLOAD => Value::Float(speed(info.uploaded, time.total) as f64),
        CURLINFO_SIZE_UPLOAD_T => Value::Int(info.uploaded),
        CURLINFO_SIZE_DOWNLOAD_T => Value::Int(info.downloaded),
        CURLINFO_SPEED_DOWNLOAD_T => Value::Int(speed(info.downloaded, time.total)),
        CURLINFO_SPEED_UPLOAD_T => Value::Int(speed(info.uploaded, time.total)),
        CURLINFO_CONTENT_LENGTH_DOWNLOAD => Value::Float(info.download_length.unwrap_or(-1) as f64),
        CURLINFO_CONTENT_LENGTH_UPLOAD => Value::Float(info.upload_length.unwrap_or(-1) as f64),
        CURLINFO_CONTENT_LENGTH_DOWNLOAD_T => Value::Int(info.download_length.unwrap_or(-1)),
        CURLINFO_CONTENT_LENGTH_UPLOAD_T => Value::Int(info.upload_length.unwrap_or(-1)),
        CURLINFO_REDIRECT_URL => url(&info.redirect_url),
        CURLINFO_PRIMARY_IP => ip(info.peer),
        CURLINFO_LOCAL_IP => ip(info.local),
        CURLINFO_PRIMARY_PORT => port(info.peer),
        CURLINFO_LOCAL_PORT => port(info.local),
        CURLINFO_HTTP_VERSION => Value::Int(info.http_version),
        CURLINFO_PROTOCOL => Value::Int(match info.scheme.as_deref() {
            Some("http") => 1,
            Some("https") => 2,
            _ => 0,
        }),
        CURLINFO_SCHEME => url(&info.scheme),
        CURLINFO_HEADER_OUT => bytes(&info.request_header),
        CURLINFO_EFFECTIVE_METHOD => Value::from(
            info.method
                .as_deref()
                .unwrap_or_else(|| easy.settings.effective_method()),
        ),
        CURLINFO_PRIVATE => easy.settings.private.clone().unwrap_or(Value::Bool(false)),
        CURLINFO_COOKIELIST => crate::cookies::list(&easy.cookies())?,
        CURLINFO_CERTINFO | CURLINFO_SSL_ENGINES => Value::from(PhpArray::new()),
        CURLINFO_CAINFO | CURLINFO_CAPATH => Value::Bool(false),
        CURLINFO_USED_PROXY => Value::Int(i64::from(info.proxy)),
        CURLINFO_HTTPAUTH_USED => Value::Int(info.auth),
        CURLINFO_HTTPAUTH_AVAIL => Value::Int(info.auth_available),
        CURLINFO_PROXYAUTH_AVAIL => Value::Int(info.proxy_auth_available),
        CURLINFO_PROXYAUTH_USED => Value::Int(info.proxy_auth),
        CURLINFO_HTTP_CONNECTCODE => Value::Int(info.proxy_connect_code),
        CURLINFO_CONN_ID => Value::Int(info.connection_id.unwrap_or(-1)),
        CURLINFO_NUM_CONNECTS => Value::Int(info.new_connections),
        CURLINFO_REFERER => bytes(&info.referer),
        CURLINFO_RETRY_AFTER => Value::Int(info.retry_after),
        CURLINFO_CONDITION_UNMET
        | CURLINFO_OS_ERRNO
        | CURLINFO_PROXY_ERROR
        | CURLINFO_PROXY_SSL_VERIFYRESULT
        | CURLINFO_RTSP_CLIENT_CSEQ
        | CURLINFO_RTSP_CSEQ_RECV
        | CURLINFO_RTSP_SERVER_CSEQ => Value::Int(0),
        _ => Value::Bool(false),
    })
}
