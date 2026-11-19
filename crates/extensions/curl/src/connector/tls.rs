//! `CURLOPT_CAINFO` replaces the trusted roots for a transfer.

use std::{
    io,
    sync::{Arc, OnceLock},
};

use rustls::{
    ClientConfig, RootCertStore,
    pki_types::{CertificateDer, pem::PemObject},
};

use crate::{
    error::TransferError,
    options::{HttpVersion, TlsVersions, Transport},
};

static ROOTS: OnceLock<RootCertStore> = OnceLock::new();

fn system_roots() -> &'static RootCertStore {
    ROOTS.get_or_init(|| {
        let mut roots = RootCertStore::empty();
        roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
        if roots.is_empty() {
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
        roots
    })
}

pub(super) async fn config(settings: &Transport) -> Result<Arc<ClientConfig>, TransferError> {
    let roots = if let Some(path) = &settings.ca_file {
        let pem = tokio::fs::read(path).await.map_err(|_| {
            TransferError::new(
                77,
                format!("error setting certificate file: {}", path.display()),
            )
        })?;
        let mut roots = RootCertStore::empty();
        roots
            .add_parsable_certificates(CertificateDer::pem_slice_iter(&pem).filter_map(Result::ok));
        if roots.is_empty() {
            return Err(TransferError::new(
                77,
                format!("error setting certificate file: {}", path.display()),
            ));
        }
        roots
    } else if let Some(roots) = ROOTS.get() {
        roots.clone()
    } else {
        tokio::task::spawn_blocking(system_roots)
            .await
            .map_err(|_| TransferError::new(77, "Cannot load trusted root certificates"))?
            .clone()
    };
    use rustls::version::{TLS12, TLS13};
    let versions: &[_] = match settings.tls {
        TlsVersions::Modern => &[&TLS13, &TLS12],
        TlsVersions::Tls12 => &[&TLS12],
        TlsVersions::Tls13 => &[&TLS13],
    };
    let mut config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(versions)
            .map_err(|error| TransferError::new(35, error.to_string()))?
            .with_root_certificates(roots)
            .with_no_client_auth();
    if settings.alpn {
        config.alpn_protocols = match settings.http {
            HttpVersion::OneZero | HttpVersion::OneOne => vec![b"http/1.1".to_vec()],
            HttpVersion::TwoPrior => vec![b"h2".to_vec()],
            _ => vec![b"h2".to_vec(), b"http/1.1".to_vec()],
        };
    }
    Ok(Arc::new(config))
}

pub(super) fn handshake_error(error: io::Error) -> TransferError {
    let tls = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<rustls::Error>());
    if let Some(rustls::Error::InvalidCertificate(problem)) = tls {
        TransferError::new(60, format!("SSL certificate problem: {problem:?}"))
    } else {
        TransferError::new(35, format!("TLS connection failed: {error}"))
    }
}
