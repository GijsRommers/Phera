//! Builds a rustls client using the transfer's trust and identity settings.

mod files;
mod verify;

use std::{
    io,
    sync::{Arc, OnceLock},
};

use rustls::{ClientConfig, RootCertStore, client::WebPkiServerVerifier};

use crate::{
    error::TransferError,
    options::{HttpVersion, TlsFiles, TlsVersions, Transport},
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
    let (roots, identity) = if settings.tls_files == TlsFiles::default()
        && let Some(roots) = ROOTS.get()
    {
        (roots.clone(), None)
    } else {
        let files = settings.tls_files.clone();
        tokio::task::spawn_blocking(move || files::load(&files))
            .await
            .map_err(|_| TransferError::new(77, "Cannot load TLS certificate configuration"))??
    };
    use rustls::version::{TLS12, TLS13};
    let versions: &[_] = match settings.tls {
        TlsVersions::Modern => &[&TLS13, &TLS12],
        TlsVersions::Tls12 => &[&TLS12],
        TlsVersions::Tls13 => &[&TLS13],
    };
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(Arc::clone(&provider))
        .with_protocol_versions(versions)
        .map_err(|error| TransferError::new(35, error.to_string()))?;
    let builder = if let Some(pins) = &settings.tls_files.pins {
        let verifier = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider)
            .build()
            .map_err(|error| TransferError::new(77, error.to_string()))?;
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verify::Pinned {
                verifier,
                pins: Arc::clone(pins),
            }))
    } else {
        builder.with_root_certificates(roots)
    };
    let mut config = if let Some((certificates, key)) = identity {
        builder
            .with_client_auth_cert(certificates, key)
            .map_err(|error| TransferError::new(58, error.to_string()))?
    } else {
        builder.with_no_client_auth()
    };
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
    } else if let Some(rustls::Error::Other(problem)) = tls
        && problem.0.is::<verify::PinMismatch>()
    {
        TransferError::new(90, problem.to_string())
    } else {
        TransferError::new(35, format!("TLS connection failed: {error}"))
    }
}
