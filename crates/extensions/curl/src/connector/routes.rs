//! `CURLOPT_RESOLVE` overrides DNS while keeping the URL host for TLS.

use std::{
    net::{IpAddr, SocketAddr},
    sync::atomic::{AtomicI64, Ordering},
    time::{Duration, Instant},
};

use socket2::{SockRef, TcpKeepalive};
use tokio::net::{TcpStream, UnixStream, lookup_host};

use crate::{
    error::TransferError,
    options::{IpVersion, Transport},
    share::SharedDns,
};

use super::{ConnectionInfo, Stream, proxy::Proxy};

static CONNECTION_IDS: AtomicI64 = AtomicI64::new(0);

pub(super) async fn connect(
    settings: &Transport,
    proxy: Option<&Proxy>,
    dns: Option<&SharedDns>,
    host: &str,
    port: u16,
) -> Result<(Stream, ConnectionInfo), TransferError> {
    let id = CONNECTION_IDS.fetch_add(1, Ordering::Relaxed);
    if let Some(path) = &settings.unix {
        let now = Instant::now();
        let socket = UnixStream::connect(path).await.map_err(|error| {
            TransferError::new(
                7,
                format!(
                    "Failed to connect to Unix socket {}: {error}",
                    path.display()
                ),
            )
        })?;
        return Ok((
            Box::new(socket),
            ConnectionInfo {
                id,
                peer: None,
                local: None,
                lookup: now,
                connected: Instant::now(),
                tls: None,
                proxy: false,
                proxy_connect_code: 0,
                proxy_auth: false,
            },
        ));
    }
    let (target, target_port) = if let Some(proxy) = proxy {
        (proxy.host.as_str(), proxy.port)
    } else {
        connect_to(settings, host, port)?
    };
    let neighbors = addresses(settings, dns, target, target_port, proxy.is_some()).await?;
    let lookup = Instant::now();
    let socket = TcpStream::connect(neighbors.as_slice())
        .await
        .map_err(|_| {
            TransferError::new(
                7,
                format!(
                    "Failed to connect to {target} port {target_port}: Could not connect to server"
                ),
            )
        })?;
    socket.set_nodelay(settings.nodelay).map_err(socket_error)?;
    if settings.keepalive {
        let keepalive = TcpKeepalive::new()
            .with_time(Duration::from_secs(settings.keep_idle))
            .with_interval(Duration::from_secs(settings.keep_interval))
            .with_retries(settings.keep_count);
        SockRef::from(&socket)
            .set_tcp_keepalive(&keepalive)
            .map_err(socket_error)?;
    }
    let info = ConnectionInfo {
        id,
        peer: socket.peer_addr().ok(),
        local: socket.local_addr().ok(),
        lookup,
        connected: Instant::now(),
        tls: None,
        proxy: proxy.is_some(),
        proxy_connect_code: 0,
        proxy_auth: proxy.is_some_and(|proxy| proxy.authorization().is_some()),
    };
    Ok((Box::new(socket), info))
}

fn socket_error(error: std::io::Error) -> TransferError {
    TransferError::new(7, format!("Could not configure socket: {error}"))
}

/// Splits curl's colon-delimited routing fields, retaining IPv6 literals.
fn split(fields: &str) -> Vec<&str> {
    let mut start = 0;
    let mut bracketed = false;
    let mut output = Vec::new();
    for (offset, byte) in fields.bytes().enumerate() {
        match byte {
            b'[' => bracketed = true,
            b']' => bracketed = false,
            b':' if !bracketed => {
                output.push(&fields[start..offset]);
                start = offset + 1;
            }
            _ => {}
        }
    }
    output.push(&fields[start..]);
    output
}

fn connect_to<'a>(
    settings: &'a Transport,
    host: &'a str,
    port: u16,
) -> Result<(&'a str, u16), TransferError> {
    for entry in &settings.connect_to {
        let fields = split(entry);
        let [from, from_port, to, to_port] = fields.as_slice() else {
            return Err(TransferError::new(
                49,
                format!("Couldn't parse CURLOPT_CONNECT_TO entry '{entry}'"),
            ));
        };
        if !from.is_empty() && !from.trim_matches(['[', ']']).eq_ignore_ascii_case(host) {
            continue;
        }
        if !from_port.is_empty() && from_port.parse::<u16>().ok() != Some(port) {
            continue;
        }
        let target = if to.is_empty() {
            host
        } else {
            to.trim_matches(['[', ']'])
        };
        let target_port = if to_port.is_empty() {
            port
        } else {
            to_port.parse::<u16>().map_err(|_| {
                TransferError::new(
                    49,
                    format!("Couldn't parse CURLOPT_CONNECT_TO entry '{entry}'"),
                )
            })?
        };
        return Ok((target, target_port));
    }
    Ok((host, port))
}

fn overrides(
    settings: &Transport,
    host: &str,
    port: u16,
) -> Result<Option<Vec<SocketAddr>>, TransferError> {
    let mut catchall = None;
    let mut matched = None;
    for entry in &settings.resolve {
        let clean = entry.trim_start_matches(['+', '-']);
        let fields = split(clean);
        if let [name, port_text] = fields.as_slice()
            && entry.starts_with('-')
        {
            if name.eq_ignore_ascii_case(host) && port_text.parse::<u16>().ok() == Some(port) {
                matched = None;
            }
            continue;
        }
        let [name, port_text, numbers] = fields.as_slice() else {
            return Err(TransferError::new(
                49,
                format!("Couldn't parse CURLOPT_RESOLVE entry '{entry}'"),
            ));
        };
        if port_text.parse::<u16>().ok() != Some(port) {
            continue;
        }
        let destination = if name.eq_ignore_ascii_case(host) {
            &mut matched
        } else if *name == "*" {
            &mut catchall
        } else {
            continue;
        };
        let ips: Result<Vec<_>, _> = numbers
            .split(',')
            .map(|address| {
                address
                    .trim_matches(['[', ']'])
                    .parse::<IpAddr>()
                    .map(|ip| SocketAddr::new(ip, port))
            })
            .collect();
        *destination = Some(ips.map_err(|_| {
            TransferError::new(
                49,
                format!("Couldn't parse CURLOPT_RESOLVE entry '{entry}'"),
            )
        })?);
    }
    Ok(matched.or(catchall))
}

async fn addresses(
    settings: &Transport,
    dns: Option<&SharedDns>,
    host: &str,
    port: u16,
    proxy: bool,
) -> Result<Vec<SocketAddr>, TransferError> {
    let cached = dns.and_then(|cache| cache.lock().ok()?.get(host, port));
    let source = if let Some(addresses) = overrides(settings, host, port)? {
        addresses
    } else if let Some(addresses) = cached {
        addresses
    } else {
        let addresses: Vec<_> = lookup_host((host, port))
            .await
            .map_err(|_| match proxy {
                true => TransferError::new(5, format!("Could not resolve proxy: {host}")),
                false => TransferError::new(6, format!("Could not resolve host: {host}")),
            })?
            .collect();
        if let Some(cache) = dns
            && let Ok(mut cache) = cache.lock()
        {
            cache.put(host, port, &addresses);
        }
        addresses
    };
    let resolved: Vec<_> = source
        .into_iter()
        .filter(|address| match settings.ip {
            IpVersion::Any => true,
            IpVersion::Four => address.is_ipv4(),
            IpVersion::Six => address.is_ipv6(),
        })
        .collect();
    if resolved.is_empty() {
        return Err(if proxy {
            TransferError::new(5, format!("Could not resolve proxy: {host}"))
        } else {
            TransferError::new(6, format!("Could not resolve host: {host}"))
        });
    }
    Ok(resolved)
}
