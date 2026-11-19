//! `CURLOPT_PROXYTYPE` selects an HTTP proxy. Unsupported transports fail visibly.

use phera_extension_api::{Context, NativeError};

use crate::constants::ids::*;
use crate::options::{HttpVersion, IpVersion, Settings, TlsVersions, path};

use super::{Change, strings, tls};

pub(super) fn change(
    context: &mut dyn Context,
    settings: &mut Settings,
    option: i64,
    number: i64,
    text: Option<Vec<u8>>,
    list: Vec<Vec<u8>>,
) -> Result<Change, NativeError> {
    let transport = &mut settings.transport;
    match option {
        CURLOPT_HTTP_VERSION => {
            transport.http = match number {
                0 => HttpVersion::Auto,
                1 => HttpVersion::OneZero,
                2 => HttpVersion::OneOne,
                3 => HttpVersion::Two,
                4 => HttpVersion::TwoTls,
                5 => HttpVersion::TwoPrior,
                _ => {
                    return Ok(Change::Refused(
                        "HTTP/3 and unknown HTTP versions are unavailable",
                    ));
                }
            }
        }
        CURLOPT_IPRESOLVE => {
            transport.ip = match number {
                0 => IpVersion::Any,
                1 => IpVersion::Four,
                2 => IpVersion::Six,
                _ => return Ok(Change::Refused("unsupported IP version")),
            }
        }
        CURLOPT_RESOLVE => transport.resolve = strings::text_list(list),
        CURLOPT_CONNECT_TO => transport.connect_to = strings::text_list(list),
        CURLOPT_PROXY => {
            let proxy = strings::text(text);
            if proxy
                .as_deref()
                .and_then(|value| value.split_once("://"))
                .is_some_and(|(scheme, _)| !scheme.eq_ignore_ascii_case("http"))
            {
                return Ok(Change::Refused("only HTTP proxies are available"));
            }
            transport.proxy = proxy;
        }
        CURLOPT_PROXYTYPE if number == 0 => {}
        CURLOPT_PROXYPORT => {
            let Ok(port) = u16::try_from(number) else {
                return Ok(Change::Refused("proxy port must be between 0 and 65535"));
            };
            transport.proxy_port = (port != 0).then_some(port);
        }
        CURLOPT_PROXYUSERPWD => transport.proxy_credentials = text,
        CURLOPT_PROXYAUTH if number == 1 || number == 0 => {
            transport.proxy_auth = number == 1;
        }
        CURLOPT_NOPROXY => transport.no_proxy = strings::text(text),
        CURLOPT_HTTPPROXYTUNNEL => transport.tunnel = number != 0,
        CURLOPT_UNIX_SOCKET_PATH => {
            transport.unix = text
                .map(|value| path(context, &value))
                .transpose()
                .map_err(|_| NativeError::value_error("Cannot resolve CURLOPT_UNIX_SOCKET_PATH"))?;
        }
        CURLOPT_PORT => settings.port = u16::try_from(number).ok().filter(|p| *p != 0),
        CURLOPT_TCP_KEEPALIVE => transport.keepalive = number != 0,
        CURLOPT_TCP_KEEPIDLE => transport.keep_idle = number.max(1) as u64,
        CURLOPT_TCP_KEEPINTVL => transport.keep_interval = number.max(1) as u64,
        CURLOPT_TCP_KEEPCNT => transport.keep_count = number.clamp(1, i64::from(u32::MAX)) as u32,
        CURLOPT_TCP_NODELAY => transport.nodelay = number != 0,
        CURLOPT_SSL_ENABLE_ALPN => transport.alpn = number != 0,
        CURLOPT_SSLVERSION => {
            let minimum = number & 0xffff;
            let maximum = number & !0xffff;
            transport.tls = match (minimum, maximum) {
                (
                    CURL_SSLVERSION_DEFAULT | CURL_SSLVERSION_TLSV1_2,
                    0 | CURL_SSLVERSION_MAX_DEFAULT | CURL_SSLVERSION_MAX_TLSV1_3,
                ) => TlsVersions::Modern,
                (
                    CURL_SSLVERSION_DEFAULT | CURL_SSLVERSION_TLSV1_2,
                    CURL_SSLVERSION_MAX_TLSV1_2,
                ) => TlsVersions::Tls12,
                (
                    CURL_SSLVERSION_TLSV1_3,
                    0 | CURL_SSLVERSION_MAX_DEFAULT | CURL_SSLVERSION_MAX_TLSV1_3,
                ) => TlsVersions::Tls13,
                _ => return Ok(Change::Refused("only TLS 1.2 and TLS 1.3 are available")),
            };
        }
        CURLOPT_INTERFACE => return Ok(Change::Refused("binding to an interface is unavailable")),
        option if tls::is_blob(option) || tls::takes_text(option) => {
            return tls::change(context, &mut transport.tls_files, option, text);
        }
        _ => return Ok(strings::change(settings, option, text)),
    }
    Ok(Change::Applied)
}
