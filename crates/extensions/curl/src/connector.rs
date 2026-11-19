//! `curl_exec($handle)` connects through a worker-local Hyper client pool.

mod proxy;
mod routes;
mod tls;

use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    io,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use hyper::Uri;
use hyper_util::{
    client::legacy::{
        Client,
        connect::{Connected, Connection},
    },
    rt::{TokioExecutor, TokioIo, TokioTimer},
};
use tokio::io::{AsyncRead, AsyncWrite};
use tower_service::Service;
use url::Url;

use crate::{
    error::TransferError,
    options::{HttpVersion, Settings, Transport},
    share::SharedDns,
    transfer::RequestBody,
};

trait Socket: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Socket for T {}
type Stream = Box<dyn Socket>;
pub(crate) type HttpClient = Client<Connector, RequestBody>;

#[derive(Clone)]
pub(crate) struct ConnectionInfo {
    pub(crate) id: i64,
    pub(crate) peer: Option<SocketAddr>,
    pub(crate) local: Option<SocketAddr>,
    pub(crate) lookup: Instant,
    pub(crate) connected: Instant,
    pub(crate) tls: Option<Instant>,
    pub(crate) proxy: bool,
    pub(crate) proxy_connect_code: i64,
    pub(crate) proxy_auth: bool,
}

pub(crate) struct Io {
    stream: Stream,
    info: ConnectionInfo,
    h2: bool,
    forward_proxy: bool,
}

impl Connection for Io {
    fn connected(&self) -> Connected {
        let mut connection = Connected::new()
            .proxy(self.forward_proxy)
            .extra(self.info.clone());
        if self.h2 {
            connection = connection.negotiated_h2();
        }
        connection
    }
}

impl AsyncRead for Io {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.stream).poll_read(cx, buffer)
    }
}

impl AsyncWrite for Io {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.stream).poll_write(cx, bytes)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.stream).poll_shutdown(cx)
    }
}

#[derive(Clone)]
pub(crate) struct Connector {
    settings: Arc<Transport>,
    tls: Option<Arc<rustls::ClientConfig>>,
    proxy: Option<proxy::Proxy>,
    timeout: Option<Duration>,
    dns: Option<SharedDns>,
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for TransferError {}

impl Service<Uri> for Connector {
    type Response = TokioIo<Io>;
    type Error = TransferError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let connector = self.clone();
        Box::pin(async move {
            let connect = connector.connect(uri);
            if let Some(timeout) = connector.timeout {
                tokio::time::timeout(timeout, connect).await.map_err(|_| {
                    TransferError::new(
                        28,
                        format!(
                            "Connection timed out after {} milliseconds",
                            timeout.as_millis()
                        ),
                    )
                })?
            } else {
                connect.await
            }
            .map(TokioIo::new)
        })
    }
}

impl Connector {
    async fn connect(&self, uri: Uri) -> Result<Io, TransferError> {
        let https = uri.scheme_str() == Some("https");
        let host = uri.host().unwrap_or_default().trim_matches(['[', ']']);
        let port = uri.port_u16().unwrap_or(if https { 443 } else { 80 });
        let tunnel = self.proxy.is_some() && (https || self.settings.tunnel);
        let (mut stream, mut info) = routes::connect(
            &self.settings,
            self.proxy.as_ref(),
            self.dns.as_ref(),
            host,
            port,
        )
        .await?;
        if tunnel && let Some(proxy) = &self.proxy {
            proxy.tunnel(&mut stream, host, port).await?;
            info.proxy_connect_code = 200;
        }
        let mut h2 = self.settings.http == HttpVersion::TwoPrior && !https;
        if https {
            let config = self
                .tls
                .clone()
                .ok_or_else(|| TransferError::new(35, "TLS is unavailable"))?;
            let server = rustls::pki_types::ServerName::try_from(host.to_owned())
                .map_err(|_| TransferError::new(3, "Invalid TLS server name"))?;
            let secure = tokio_rustls::TlsConnector::from(config)
                .connect(server, stream)
                .await
                .map_err(tls::handshake_error)?;
            h2 = secure.get_ref().1.alpn_protocol() == Some(b"h2");
            info.tls = Some(Instant::now());
            stream = Box::new(secure);
        }
        Ok(Io {
            stream,
            info,
            h2,
            forward_proxy: self.proxy.is_some() && !tunnel,
        })
    }
}

#[derive(PartialEq, Eq)]
struct PoolKey {
    target: String,
    settings: Transport,
    proxy: Option<proxy::Proxy>,
    dns: Option<usize>,
    timeout: Option<Duration>,
    idle: usize,
    scope: u64,
}

type WorkerPool = VecDeque<(PoolKey, HttpClient)>;

thread_local! {
    static WORKER: RefCell<WorkerPool> = const { RefCell::new(VecDeque::new()) };
}

pub(crate) async fn client(
    settings: &Settings,
    url: &Url,
    dns: Option<SharedDns>,
) -> Result<(HttpClient, Option<Vec<u8>>), TransferError> {
    let proxy = if settings.transport.unix.is_some() {
        if settings
            .transport
            .proxy
            .as_ref()
            .is_some_and(|proxy| !proxy.is_empty())
        {
            return Err(TransferError::new(
                4,
                "An HTTP proxy cannot use a Unix socket in Phera",
            ));
        }
        None
    } else {
        proxy::select(&settings.transport, url)?
    };
    let forward_auth = proxy
        .as_ref()
        .filter(|_| url.scheme() == "http" && !settings.transport.tunnel)
        .and_then(proxy::Proxy::authorization);
    let key = PoolKey {
        target: url.origin().ascii_serialization(),
        settings: settings.transport.clone(),
        proxy: proxy.clone(),
        dns: dns.as_ref().map(|dns| Arc::as_ptr(dns) as usize),
        timeout: settings.connect_timeout,
        idle: settings.pool_idle,
        scope: settings.pool_scope,
    };
    let h2 = matches!(settings.transport.http, HttpVersion::TwoPrior)
        || url.scheme() == "https"
            && !matches!(
                settings.transport.http,
                HttpVersion::OneZero | HttpVersion::OneOne
            );
    let reusable = !(settings.fresh || settings.forbid_reuse || (h2 && !settings.multiplex));
    if reusable
        && let Some(client) = WORKER.with(|pool| {
            let mut pool = pool.borrow_mut();
            let index = pool.iter().position(|(existing, _)| *existing == key)?;
            let entry = pool.remove(index)?;
            let client = entry.1.clone();
            pool.push_back(entry);
            Some(client)
        })
    {
        return Ok((client, forward_auth));
    }
    let tls = if url.scheme() == "https" {
        Some(tls::config(&settings.transport).await?)
    } else {
        None
    };
    let mut builder = Client::builder(TokioExecutor::new());
    builder
        .pool_timer(TokioTimer::new())
        .pool_idle_timeout(Duration::from_secs(45))
        .pool_max_idle_per_host(if reusable { settings.pool_idle } else { 0 });
    if settings.transport.http == HttpVersion::TwoPrior {
        builder.http2_only(true);
    }
    let client = builder.build(Connector {
        settings: Arc::new(settings.transport.clone()),
        tls,
        proxy,
        timeout: settings.connect_timeout,
        dns,
    });
    if reusable {
        WORKER.with(|pool| {
            let mut pool = pool.borrow_mut();
            if pool.len() >= 32 {
                pool.pop_front();
            }
            pool.push_back((key, client.clone()));
        });
    }
    Ok((client, forward_auth))
}
