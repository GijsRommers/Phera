//! `CURLOPT_PROXY` forwards HTTP requests and tunnels HTTPS with CONNECT.

use base64::{Engine, engine::general_purpose::STANDARD};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

use crate::{error::TransferError, options::Transport};

use super::Stream;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Proxy {
    pub(super) host: String,
    pub(super) port: u16,
    credentials: Option<Vec<u8>>,
}

pub(super) fn select(settings: &Transport, target: &Url) -> Result<Option<Proxy>, TransferError> {
    let variable = |prefix: &str| std::env::var(prefix).ok();
    let bypass = settings
        .no_proxy
        .clone()
        .or_else(|| variable("no_proxy"))
        .or_else(|| variable("NO_PROXY"));
    let host = target
        .host_str()
        .unwrap_or_default()
        .trim_matches(['[', ']']);
    if bypass.as_deref().is_some_and(|value| no_proxy(value, host)) {
        return Ok(None);
    }

    let address = settings
        .proxy
        .clone()
        .or_else(|| {
            if target.scheme() == "https" {
                variable("https_proxy").or_else(|| variable("HTTPS_PROXY"))
            } else {
                variable("http_proxy")
            }
        })
        .or_else(|| variable("all_proxy"))
        .or_else(|| variable("ALL_PROXY"));
    let Some(address) = address.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let address = if address.contains("://") {
        address
    } else {
        format!("http://{address}")
    };
    // WHATWG URL parsing removes an explicit default port.
    let supplied_port = address
        .split_once("://")
        .and_then(|(_, address)| address.split('/').next())
        .and_then(|authority| authority.rsplit_once(':'))
        .and_then(|(_, port)| port.parse::<u16>().ok());
    let valid = Url::parse(&address).ok();
    let Some(url) = valid.filter(|url| url.scheme() == "http") else {
        return Err(TransferError::new(
            7,
            "Unsupported or malformed proxy address",
        ));
    };
    let Some(host) = url.host_str() else {
        return Err(TransferError::new(5, "Could not resolve proxy"));
    };
    let credentials = settings
        .proxy_auth
        .then(|| {
            settings.proxy_credentials.clone().or_else(|| {
                if url.username().is_empty() {
                    return None;
                }
                Some(
                    format!("{}:{}", url.username(), url.password().unwrap_or_default())
                        .into_bytes(),
                )
            })
        })
        .flatten();
    Ok(Some(Proxy {
        host: host.trim_matches(['[', ']']).to_owned(),
        port: settings
            .proxy_port
            .or(url.port())
            .or(supplied_port)
            .unwrap_or(1080),
        credentials,
    }))
}

fn no_proxy(list: &str, host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    list.split(',').map(str::trim).any(|item| {
        let item = item
            .trim_matches(['[', ']'])
            .trim_matches('.')
            .to_ascii_lowercase();
        if item == "*" || item == host {
            return true;
        }
        if let Some((network, bits)) = item.split_once('/') {
            return address_in_network(&host, network, bits);
        }
        !item.is_empty()
            && host
                .strip_suffix(&item)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

fn address_in_network(ip: &str, network: &str, bits: &str) -> bool {
    use std::net::IpAddr;
    let Ok(bits) = bits.parse::<u32>() else {
        return false;
    };
    match (ip.parse::<IpAddr>(), network.parse::<IpAddr>()) {
        (Ok(IpAddr::V4(ip)), Ok(IpAddr::V4(network))) if bits <= 32 => {
            bits == 0 || u32::from(ip) >> (32 - bits) == u32::from(network) >> (32 - bits)
        }
        (Ok(IpAddr::V6(ip)), Ok(IpAddr::V6(network))) if bits <= 128 => {
            bits == 0 || u128::from(ip) >> (128 - bits) == u128::from(network) >> (128 - bits)
        }
        _ => false,
    }
}

impl Proxy {
    pub(super) fn authorization(&self) -> Option<Vec<u8>> {
        self.credentials
            .as_ref()
            .map(|value| format!("Basic {}", STANDARD.encode(value)).into_bytes())
    }

    pub(super) async fn tunnel(
        &self,
        stream: &mut Stream,
        host: &str,
        port: u16,
    ) -> Result<(), TransferError> {
        let authority = if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let mut request = format!(
            "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Connection: Keep-Alive\r\n"
        )
        .into_bytes();
        if let Some(auth) = self.authorization() {
            request.extend_from_slice(b"Proxy-Authorization: ");
            request.extend_from_slice(&auth);
            request.extend_from_slice(b"\r\n");
        }
        request.extend_from_slice(b"\r\n");
        let failure =
            |error: std::io::Error| TransferError::new(56, format!("CONNECT failed: {error}"));
        stream.write_all(&request).await.map_err(failure)?;
        let mut response = Vec::new();
        while response.len() < 64 * 1024 {
            response.push(stream.read_u8().await.map_err(failure)?);
            if response.ends_with(b"\r\n\r\n") {
                let status = response
                    .split(|byte| *byte == b' ')
                    .nth(1)
                    .and_then(|bytes| std::str::from_utf8(bytes).ok())
                    .and_then(|code| code.parse::<u16>().ok())
                    .unwrap_or(0);
                if (200..300).contains(&status) {
                    return Ok(());
                }
                return Err(TransferError::new(
                    56,
                    format!("CONNECT tunnel failed, response {status}"),
                ));
            }
        }
        Err(TransferError::new(56, "CONNECT response headers too large"))
    }
}
