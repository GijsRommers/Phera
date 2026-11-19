//! PostgreSQL negotiates TLS over the same TCP socket with an SSLRequest.
//! `sslmode=verify-full;sslrootcert=ca.pem` requires TLS and verifies the name.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustls::pki_types::{PrivateKeyDer, ServerName, pem::PemObject};
use rustls::{ClientConfig, RootCertStore};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use super::connect::{failed, tls_failure};
use super::stream::Socket;
use super::verify::VerifyChain;
use crate::drivers::{connection_error, tls};
use crate::dsn::Dsn;
use crate::errors::DbError;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Disable,
    Allow,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl Mode {
    pub(super) fn parse(value: &str) -> Result<Self, DbError> {
        match value {
            "disable" => Ok(Self::Disable),
            "allow" => Ok(Self::Allow),
            "prefer" | "" => Ok(Self::Prefer),
            "require" => Ok(Self::Require),
            "verify-ca" => Ok(Self::VerifyCa),
            "verify-full" => Ok(Self::VerifyFull),
            _ => Err(connection_error(format!(
                "invalid sslmode value: \"{value}\""
            ))),
        }
    }
}

pub(super) async fn negotiate(
    mut socket: TcpStream,
    dsn: &Dsn,
    host: &str,
    port: u16,
    mode: Mode,
) -> Result<Box<dyn Socket>, DbError> {
    if matches!(mode, Mode::Disable | Mode::Allow) {
        return Ok(Box::new(socket));
    }
    socket
        .write_all(&[0, 0, 0, 8, 4, 210, 22, 47])
        .await
        .map_err(|error| failed(host, port, error))?;
    let reply = socket
        .read_u8()
        .await
        .map_err(|error| failed(host, port, error))?;
    match reply {
        b'N' if mode == Mode::Prefer => return Ok(Box::new(socket)),
        b'N' => {
            return Err(failed(
                host,
                port,
                "server does not support SSL, but SSL was required",
            ));
        }
        b'S' => {}
        reply => {
            return Err(failed(
                host,
                port,
                format!(
                    "received invalid response to SSL negotiation: {}",
                    char::from(reply)
                ),
            ));
        }
    }

    let config = client_config(dsn, mode)
        .await
        .map_err(|error| failed(host, port, error))?;
    let name = ServerName::try_from(host.to_owned())
        .map_err(|_| failed(host, port, format!("invalid server name \"{host}\"")))?;
    let stream = TlsConnector::from(Arc::new(config))
        .connect(name, socket)
        .await
        .map_err(|error| tls_failure(host, port, error))?;
    Ok(Box::new(stream))
}

async fn client_config(dsn: &Dsn, mode: Mode) -> Result<ClientConfig, String> {
    let provider = Arc::new(rustls_rustcrypto::provider());
    let algorithms = provider.signature_verification_algorithms;
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| error.to_string())?;
    let root_file = dsn.get("sslrootcert", "");
    let roots = match root_file {
        "" if !matches!(mode, Mode::VerifyCa | Mode::VerifyFull) => None,
        "" | "system" => Some(tls::system_store().await),
        file => Some(read_roots(file).await?),
    };
    let builder = match (mode, roots) {
        (Mode::VerifyFull, Some(roots)) => builder.with_root_certificates(roots),
        (_, roots) => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(VerifyChain {
                roots: roots.map(Arc::new),
                algorithms,
            })),
    };
    let Some(cert) = client_file(dsn, "sslcert", "postgresql.crt").await else {
        return Ok(builder.with_no_client_auth());
    };
    let chain = match tokio::fs::read(&cert).await {
        Ok(chain) => chain,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(builder.with_no_client_auth());
        }
        Err(error) => {
            return Err(format!(
                "could not open certificate file \"{}\": {error}",
                cert.display()
            ));
        }
    };
    let key = client_file(dsn, "sslkey", "postgresql.key")
        .await
        .ok_or_else(|| "certificate present, but private key file is missing".to_owned())?;
    let chain = tls::certificates(&chain).map_err(|error| error.to_string())?;
    let key = private_key(&key).await?;
    builder
        .with_client_auth_cert(chain, key)
        .map_err(|error| error.to_string())
}

async fn read_roots(file: &str) -> Result<RootCertStore, String> {
    let bytes = tokio::fs::read(file)
        .await
        .map_err(|error| format!("could not read root certificate file \"{file}\": {error}"))?;
    let mut roots = RootCertStore::empty();
    for cert in tls::certificates(&bytes).map_err(|error| error.to_string())? {
        roots.add(cert).map_err(|error| error.to_string())?;
    }
    if roots.is_empty() {
        return Err(format!(
            "could not read root certificate file \"{file}\": no certificate found"
        ));
    }
    Ok(roots)
}

async fn client_file(dsn: &Dsn, field: &str, filename: &str) -> Option<PathBuf> {
    let explicit = dsn.get(field, "");
    if !explicit.is_empty() {
        return Some(PathBuf::from(explicit));
    }
    let path = PathBuf::from(std::env::var_os("HOME")?)
        .join(".postgresql")
        .join(filename);
    tokio::fs::try_exists(&path).await.ok()?.then_some(path)
}

async fn private_key(path: &Path) -> Result<PrivateKeyDer<'static>, String> {
    let mut file = tokio::fs::File::open(path).await.map_err(|error| {
        format!(
            "could not open private key file \"{}\": {error}",
            path.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata().await.map_err(|error| error.to_string())?;
        let forbidden = if metadata.uid() == 0 { 0o037 } else { 0o077 };
        if metadata.mode() & forbidden != 0 {
            let file = path.display();
            return Err(format!(
                "private key file \"{file}\" has group or world access; file must \
                have permissions u=rw (0600) or less if owned by the current user, or permissions \
                u=rw,g=r (0640) or less if owned by root"
            ));
        }
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    PrivateKeyDer::from_pem_slice(&bytes).map_err(|error| error.to_string())
}
