//! Sending one request: pooled connections, TLS, redirects and timeouts.
//!
//! Each worker thread keeps one Hyper client per connection setting
//! (`verify`, `http2`, connect timeout), shared by every
//! `Phera\Http\Client` on the thread and every request it serves. A client
//! keeps idle HTTP/1.1 connections open for reuse and multiplexes HTTP/2
//! requests over one connection per origin. Nothing here touches PHP, so
//! the requests of `pool()` run at the same time.

use std::cell::RefCell;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use http::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HOST, LOCATION};
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tower_service::Service;
use url::Url;

/// How connections are made; one pooled client per distinct setting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Transport {
    pub(super) verify: bool,
    /// Offer HTTP/2 to TLS servers (ALPN).
    pub(super) http2: bool,
    /// Speak HTTP/2 to `http://` servers without asking (h2c).
    pub(super) http2_prior_knowledge: bool,
    pub(super) connect_timeout: Option<Duration>,
}

/// One request, ready to go.
pub(super) struct Prepared {
    pub(super) method: Method,
    pub(super) url: Url,
    pub(super) headers: HeaderMap,
    pub(super) body: Bytes,
    pub(super) timeout: Option<Duration>,
    pub(super) max_redirects: u32,
}

/// What came back from the last request of a redirect chain.
pub(super) struct Received {
    pub(super) status: u16,
    pub(super) headers: HeaderMap,
    pub(super) body: Bytes,
}

/// Why a request got no response; becomes a `Phera\Http\ClientException`.
#[derive(Debug)]
pub(super) struct ClientError(pub(super) String);

type HttpClient = Client<Connector, Full<Bytes>>;

thread_local! {
    static CLIENTS: RefCell<Vec<(Transport, HttpClient)>> = const { RefCell::new(Vec::new()) };
}

/// The thread's client for `transport`, made on first use.
pub(super) async fn client(transport: Transport) -> Result<HttpClient, ClientError> {
    if let Some(client) = CLIENTS.with(|clients| {
        clients
            .borrow()
            .iter()
            .find(|(existing, _)| *existing == transport)
            .map(|(_, client)| client.clone())
    }) {
        return Ok(client);
    }
    let tls = tls_config(transport).await?;
    let mut builder = Client::builder(TokioExecutor::new());
    builder
        .pool_timer(TokioTimer::new())
        .pool_idle_timeout(Duration::from_secs(90))
        .timer(TokioTimer::new());
    if transport.http2_prior_knowledge {
        builder.http2_only(true);
    }
    let client = builder.build(Connector { transport, tls });
    CLIENTS.with(|clients| {
        clients.borrow_mut().push((transport, client.clone()));
    });
    Ok(client)
}

/// Sends `request`, following redirects, within its timeout.
pub(super) async fn send(client: HttpClient, request: Prepared) -> Result<Received, ClientError> {
    match request.timeout {
        Some(limit) => tokio::time::timeout(limit, follow(&client, request))
            .await
            .map_err(|_| {
                ClientError(format!("Request timed out after {} ms", limit.as_millis()))
            })?,
        None => follow(&client, request).await,
    }
}

async fn follow(client: &HttpClient, mut request: Prepared) -> Result<Received, ClientError> {
    let mut redirects = 0;
    loop {
        let received = send_once(client, &request).await?;
        let status = StatusCode::from_u16(received.status).unwrap_or(StatusCode::OK);
        let location = received
            .headers
            .get(LOCATION)
            .and_then(|location| location.to_str().ok());
        let (true, Some(location)) = (status.is_redirection(), location) else {
            return Ok(received);
        };
        if request.max_redirects == 0 {
            return Ok(received);
        }
        if redirects == request.max_redirects {
            return Err(ClientError(format!(
                "Will not follow more than {} redirects (last went to {location})",
                request.max_redirects
            )));
        }
        redirects += 1;
        let next = request.url.join(location).map_err(|error| {
            ClientError(format!("Invalid redirect location \"{location}\": {error}"))
        })?;
        redirect(&mut request, status, next);
    }
}

/// The next request of a redirect: 303 (and 301 or 302 after anything
/// but GET or HEAD) turns into a GET without a body, as browsers do; 307
/// and 308 repeat the request. Credentials stay with their origin.
fn redirect(request: &mut Prepared, status: StatusCode, next: Url) {
    let becomes_get = status == StatusCode::SEE_OTHER
        || (matches!(status.as_u16(), 301 | 302)
            && request.method != Method::GET
            && request.method != Method::HEAD);
    if becomes_get {
        if request.method != Method::HEAD {
            request.method = Method::GET;
        }
        request.body = Bytes::new();
        request.headers.remove(CONTENT_TYPE);
        request.headers.remove(CONTENT_LENGTH);
    }
    if next.origin() != request.url.origin() {
        request.headers.remove(AUTHORIZATION);
        request.headers.remove(COOKIE);
        request.headers.remove(HOST);
    }
    request.url = next;
}

async fn send_once(client: &HttpClient, request: &Prepared) -> Result<Received, ClientError> {
    let uri: Uri = request
        .url
        .as_str()
        .parse()
        .map_err(|error| ClientError(format!("Invalid URI \"{}\": {error}", request.url)))?;
    let mut builder = http::Request::builder()
        .method(request.method.clone())
        .uri(uri);
    if let Some(headers) = builder.headers_mut() {
        headers.clone_from(&request.headers);
    }
    let outgoing = builder
        .body(Full::new(request.body.clone()))
        .map_err(|error| ClientError(error.to_string()))?;
    let response = client
        .request(outgoing)
        .await
        .map_err(|error| failure(&request.url, &error))?;
    let (parts, body) = response.into_parts();
    let body = body
        .collect()
        .await
        .map_err(|error| failure(&request.url, &error))?
        .to_bytes();
    Ok(Received {
        status: parts.status.as_u16(),
        headers: parts.headers,
        body,
    })
}

/// `GET http://127.0.0.1:1/ failed: client error (Connect): Connection refused`
/// with every cause in the chain.
fn failure(url: &Url, error: &(dyn std::error::Error + 'static)) -> ClientError {
    let mut message = format!("Request to {url} failed: {error}");
    let mut cause = error.source();
    while let Some(next) = cause {
        message.push_str(": ");
        message.push_str(&next.to_string());
        cause = next.source();
    }
    ClientError(message)
}

/// The TLS settings of `transport`, built once per process: loading the
/// system's roots takes a moment.
async fn tls_config(transport: Transport) -> Result<Arc<rustls::ClientConfig>, ClientError> {
    use tokio::sync::OnceCell;
    // Indexed by `verify`, then `http2`.
    static CONFIGS: [[OnceCell<Arc<rustls::ClientConfig>>; 2]; 2] = [
        [OnceCell::const_new(), OnceCell::const_new()],
        [OnceCell::const_new(), OnceCell::const_new()],
    ];
    let cell = &CONFIGS[usize::from(transport.verify)][usize::from(transport.http2)];
    cell.get_or_try_init(|| async {
        let builder = phera_io_pool::tls::client_builder()
            .map_err(|error| ClientError(format!("TLS is unavailable: {error}")))?;
        let mut config = if transport.verify {
            builder
                .with_root_certificates(phera_io_pool::tls::system_store().await)
                .with_no_client_auth()
        } else {
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AnyCertificate))
                .with_no_client_auth()
        };
        config.alpn_protocols = if transport.http2 {
            vec![b"h2".to_vec(), b"http/1.1".to_vec()]
        } else {
            vec![b"http/1.1".to_vec()]
        };
        Ok(Arc::new(config))
    })
    .await
    .cloned()
}

/// `'verify' => false`: any certificate is accepted (signatures are still
/// checked, so the handshake itself is sound).
#[derive(Debug)]
struct AnyCertificate;

impl rustls::client::danger::ServerCertVerifier for AnyCertificate {
    fn verify_server_cert(
        &self,
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &rustls::pki_types::CertificateDer<'_>,
        signature: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            certificate,
            signature,
            &phera_io_pool::tls::provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &rustls::pki_types::CertificateDer<'_>,
        signature: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            certificate,
            signature,
            &phera_io_pool::tls::provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        phera_io_pool::tls::provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Opens TCP connections, with TLS for `https://`.
#[derive(Clone)]
pub(super) struct Connector {
    transport: Transport,
    tls: Arc<rustls::ClientConfig>,
}

impl Service<Uri> for Connector {
    type Response = TokioIo<Stream>;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = io::Result<TokioIo<Stream>>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let connector = self.clone();
        Box::pin(async move {
            let connect = connector.connect(uri);
            let stream = match connector.transport.connect_timeout {
                Some(limit) => tokio::time::timeout(limit, connect).await.map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("connect timed out after {} ms", limit.as_millis()),
                    )
                })??,
                None => connect.await?,
            };
            Ok(TokioIo::new(stream))
        })
    }
}

impl Connector {
    async fn connect(&self, uri: Uri) -> io::Result<Stream> {
        let https = uri.scheme_str() == Some("https");
        let host = uri.host().unwrap_or_default().trim_matches(['[', ']']);
        let port = uri.port_u16().unwrap_or(if https { 443 } else { 80 });
        let socket = TcpStream::connect((host, port)).await?;
        let _ = socket.set_nodelay(true);
        if !https {
            return Ok(Stream::Plain {
                socket,
                h2: self.transport.http2_prior_knowledge,
            });
        }
        let name = ServerName::try_from(host.to_owned())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let secure = tokio_rustls::TlsConnector::from(self.tls.clone())
            .connect(name, socket)
            .await?;
        Ok(Stream::Tls(Box::new(secure)))
    }
}

/// A connection: plain TCP or TLS over TCP.
pub(super) enum Stream {
    Plain { socket: TcpStream, h2: bool },
    Tls(Box<TlsStream<TcpStream>>),
}

impl Connection for Stream {
    fn connected(&self) -> Connected {
        let h2 = match self {
            Stream::Plain { h2, .. } => *h2,
            Stream::Tls(stream) => stream.get_ref().1.alpn_protocol() == Some(b"h2"),
        };
        if h2 {
            Connected::new().negotiated_h2()
        } else {
            Connected::new()
        }
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain { socket, .. } => Pin::new(socket).poll_read(context, buffer),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_read(context, buffer),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Plain { socket, .. } => Pin::new(socket).poll_write(context, bytes),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_write(context, bytes),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain { socket, .. } => Pin::new(socket).poll_flush(context),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_flush(context),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain { socket, .. } => Pin::new(socket).poll_shutdown(context),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_shutdown(context),
        }
    }
}
