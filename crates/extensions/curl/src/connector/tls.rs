//! Builds a rustls client using the transfer's trust and identity settings.

mod files;
mod verify;

use std::{io, sync::Arc};

use phera_io_pool::tls as shared;
use rustls::{
    ClientConfig, RootCertStore,
    version::{TLS12, TLS13},
};
use tokio::sync::OnceCell;

use crate::{
    error::TransferError,
    options::{HttpVersion, TlsFiles, TlsVersions, Transport},
};

static ROOTS: OnceCell<RootCertStore> = OnceCell::const_new();

pub(super) async fn config(settings: &Transport) -> Result<Arc<ClientConfig>, TransferError> {
    let explicit = &settings.tls_files;
    let roots = if settings.verify_peer && explicit.ca_file.is_none() && explicit.ca_blob.is_none()
    {
        ROOTS.get_or_init(shared::system_store).await.clone()
    } else {
        RootCertStore::empty()
    };
    let (roots, identity) = if *explicit == TlsFiles::default() {
        (roots, None)
    } else {
        let files = explicit.clone();
        let verify_peer = settings.verify_peer;
        tokio::task::spawn_blocking(move || files::load(&files, verify_peer, roots))
            .await
            .map_err(|_| TransferError::new(77, "Cannot load TLS certificate configuration"))??
    };
    let provider = shared::provider();
    let builder =
        match settings.tls {
            TlsVersions::Modern => shared::client_builder(),
            TlsVersions::Tls12 => ClientConfig::builder_with_provider(provider.clone())
                .with_protocol_versions(&[&TLS12]),
            TlsVersions::Tls13 => ClientConfig::builder_with_provider(provider.clone())
                .with_protocol_versions(&[&TLS13]),
        };
    let builder = builder.map_err(|error| TransferError::new(35, error.to_string()))?;
    let builder =
        if settings.verify_peer && settings.verify_host && settings.tls_files.pins.is_none() {
            builder.with_root_certificates(roots)
        } else {
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(verify::Verifier {
                    roots: settings.verify_peer.then_some(roots),
                    verify_host: settings.verify_host,
                    pins: settings.tls_files.pins.clone(),
                    algorithms: provider.signature_verification_algorithms,
                }))
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
