//! HTTP/2 channels, one per endpoint and worker thread, shared by every
//! `Client` and every request on that thread.
//!
//! HTTP/2 multiplexes calls, so one connection carries all the concurrent
//! calls of a worker to an endpoint. The pool is a short list scanned
//! linearly: a program talks to a handful of endpoints. A channel whose
//! connection has closed is replaced on its next use.

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use h2::client::SendRequest;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

use crate::status::{Status, UNAVAILABLE};

thread_local! {
    static CHANNELS: RefCell<Vec<(Box<str>, SendRequest<Bytes>)>> = const { RefCell::new(Vec::new()) };
}

/// Where and how to connect.
pub struct Endpoint<'a> {
    /// Pool key: scheme, authority and TLS trust.
    pub key: &'a str,
    /// `host:port`.
    pub address: &'a str,
    /// The TLS server name.
    pub host: &'a str,
    pub tls: Option<Arc<rustls::ClientConfig>>,
    pub connect_timeout: Duration,
}

/// A channel that can start a call now, connecting if needed.
pub async fn ready(endpoint: &Endpoint<'_>) -> Result<SendRequest<Bytes>, Status> {
    if let Some(sender) = pooled(endpoint.key) {
        match sender.ready().await {
            Ok(sender) => return Ok(sender),
            Err(_) => forget(endpoint.key),
        }
    }
    let sender = connect(endpoint).await?;
    CHANNELS.with(|channels| {
        channels
            .borrow_mut()
            .push((endpoint.key.into(), sender.clone()))
    });
    sender
        .ready()
        .await
        .map_err(|error| Status::new(UNAVAILABLE, format!("connection failed: {error}")))
}

fn pooled(key: &str) -> Option<SendRequest<Bytes>> {
    CHANNELS.with(|channels| {
        channels
            .borrow()
            .iter()
            .find(|(pooled, _)| &**pooled == key)
            .map(|(_, sender)| sender.clone())
    })
}

/// Drops the pooled channel; its connection closes once calls on it end.
pub fn forget(key: &str) {
    CHANNELS.with(|channels| channels.borrow_mut().retain(|(pooled, _)| &**pooled != key));
}

async fn connect(endpoint: &Endpoint<'_>) -> Result<SendRequest<Bytes>, Status> {
    let unavailable = |error: String| {
        Status::new(
            UNAVAILABLE,
            format!("cannot connect to {}: {error}", endpoint.address),
        )
    };
    let socket = tokio::time::timeout(
        endpoint.connect_timeout,
        TcpStream::connect(endpoint.address),
    )
    .await
    .map_err(|_| unavailable("timed out".into()))?
    .map_err(|error| unavailable(error.to_string()))?;
    let _ = socket.set_nodelay(true);
    match &endpoint.tls {
        Some(config) => {
            let name = ServerName::try_from(endpoint.host.to_owned())
                .map_err(|error| unavailable(error.to_string()))?;
            let stream = tokio_rustls::TlsConnector::from(config.clone())
                .connect(name, socket)
                .await
                .map_err(|error| unavailable(error.to_string()))?;
            handshake(stream).await.map_err(unavailable)
        }
        None => handshake(socket).await.map_err(unavailable),
    }
}

/// The connection runs as a task of its own on the thread's runtime, so it
/// answers pings and window updates between calls too.
async fn handshake<T>(io: T) -> Result<SendRequest<Bytes>, String>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (sender, connection) = h2::client::Builder::new()
        .initial_window_size(1 << 20)
        .initial_connection_window_size(1 << 21)
        .handshake::<T, Bytes>(io)
        .await
        .map_err(|error| error.to_string())?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    Ok(sender)
}
