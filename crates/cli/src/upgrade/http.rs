//! HTTPS downloads with the same Hyper client and rustls policy as Phera's I/O.

use std::time::Duration;

use http_body_util::{BodyExt, Empty, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::client::conn::http1;
use hyper::{Request, Response, header};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use url::{Position, Url};

use super::Result;

pub(super) struct Client {
    tls: TlsConnector,
}

impl Client {
    pub(super) async fn new() -> Result<Self> {
        let config = phera_io_pool::tls::client_builder()?
            .with_root_certificates(phera_io_pool::tls::system_store().await)
            .with_no_client_auth();
        Ok(Self {
            tls: TlsConnector::from(std::sync::Arc::new(config)),
        })
    }

    pub(super) async fn get(&self, url: Url, limit: usize) -> Result<Bytes> {
        let host = url.host_str().unwrap_or("release server").to_owned();
        tokio::time::timeout(Duration::from_secs(120), self.download(url, limit))
            .await
            .map_err(|_| format!("Download from {host} timed out."))?
    }

    async fn download(&self, mut url: Url, limit: usize) -> Result<Bytes> {
        for _ in 0..=5 {
            let response = self.request(&url).await?;
            let status = response.status();
            if status.is_success() {
                let body = Limited::new(response.into_body(), limit).collect().await?;
                return Ok(body.to_bytes());
            }
            if !status.is_redirection() {
                return Err(format!("Could not download {} (HTTP {status}).", url.path()).into());
            }
            let location = response
                .headers()
                .get(header::LOCATION)
                .ok_or("Redirect has no destination.")?;
            url = url.join(location.to_str()?)?;
            validate_redirect(&url)?;
        }
        Err("Too many release download redirects.".into())
    }

    async fn request(&self, url: &Url) -> Result<Response<Incoming>> {
        let host = url.host_str().ok_or("The release URL has no host.")?;
        let tcp = TcpStream::connect((host, url.port().unwrap_or(443))).await?;
        let stream = self.tls.connect(host.to_owned().try_into()?, tcp).await?;
        let (mut sender, connection) = http1::handshake(TokioIo::new(stream)).await?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let request = Request::get(&url[Position::BeforePath..Position::AfterQuery])
            .header(
                header::HOST,
                &url[Position::BeforeHost..Position::AfterPort],
            )
            .header(
                header::USER_AGENT,
                concat!("Phera/", env!("CARGO_PKG_VERSION")),
            )
            .header(header::CONNECTION, "close")
            .body(Empty::<Bytes>::new())?;
        Ok(sender.send_request(request).await?)
    }
}

pub(super) fn https_url(value: &str) -> Result<Url> {
    let url = Url::parse(value)?;
    if url.scheme() != "https"
        || !url.has_host()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Use an HTTPS release base URL without credentials, a query or a fragment.".into(),
        );
    }
    Ok(url)
}

fn validate_redirect(url: &Url) -> Result<()> {
    if url.scheme() != "https"
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(
            url.host_str(),
            Some(
                "github.com"
                    | "api.github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
                    | "github-releases.githubusercontent.com"
            )
        )
    {
        return Err("Refused a release redirect outside GitHub HTTPS hosts.".into());
    }
    Ok(())
}
