//! `stream_socket_enable_crypto($socket, true, STREAM_CRYPTO_METHOD_TLS_CLIENT)` upgrades TCP.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use phera_extension_api::{ArrayKey, PhpArray, Value, value::to_bool};
use rustls::{
    DigitallySignedStruct, Error, RootCertStore, SignatureScheme,
    client::{
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        verify_server_cert_signed_by_trust_anchor, verify_server_name,
    },
    crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature},
    pki_types::{CertificateDer, ServerName, UnixTime, pem::PemObject},
    server::ParsedCertificate,
};
use tokio::io::AsyncWriteExt;
use tokio_rustls::TlsConnector;

use super::Io;

pub(super) async fn enable(io: &Io, options: &PhpArray, requested_method: i64) -> io::Result<()> {
    let mut secure = io.socket.secure.lock().await;
    if secure.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "SSL/TLS already set-up for this stream",
        ));
    }
    let name = string(options, "peer_name")
        .or_else(|| io.socket.peer.get().cloned())
        .unwrap_or_default();
    let name = ServerName::try_from(name).map_err(invalid)?;
    let mut versions = Vec::new();
    if requested_method & 64 != 0 {
        versions.push(&rustls::version::TLS13);
    }
    if requested_method & 32 != 0 {
        versions.push(&rustls::version::TLS12);
    }
    if versions.is_empty() || requested_method & 1 == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "No supported SSL/TLS client protocol",
        ));
    }
    let provider = phera_io_pool::tls::provider();
    let builder = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&versions)
        .map_err(invalid)?;
    let verify_peer = option(options, "verify_peer").is_none_or(to_bool);
    let verify_name = option(options, "verify_peer_name").is_none_or(to_bool);
    let root_store = if verify_peer {
        Some(roots(options).await?)
    } else {
        None
    };
    let mut config = if let Some(roots) = root_store.as_ref().filter(|_| verify_name) {
        builder
            .with_root_certificates(roots.clone())
            .with_no_client_auth()
    } else {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Verifier {
                roots: root_store,
                verify_name,
                algorithms: provider.signature_verification_algorithms,
            }))
            .with_no_client_auth()
    };
    config.enable_sni = option(options, "SNI_enabled").is_none_or(to_bool);
    let connector = TlsConnector::from(Arc::new(config));
    let upgraded = io
        .wait(connector.connect(name, io.socket.wire.clone()))
        .await?;
    *secure = Some(upgraded);
    Ok(())
}

pub(super) async fn disable(io: &Io) {
    let mut secure = io.socket.secure.lock().await;
    if let Some(mut stream) = secure.take() {
        stream.get_mut().1.send_close_notify();
        let _ = io.wait(stream.flush()).await;
    }
}

fn option<'a>(options: &'a PhpArray, key: &str) -> Option<&'a Value> {
    options.get(&ArrayKey::str(key))
}

fn string(options: &PhpArray, key: &str) -> Option<String> {
    let Value::String(value) = &*option(options, key)?.deref() else {
        return None;
    };
    std::str::from_utf8(value.as_bytes())
        .ok()
        .map(str::to_owned)
}

async fn roots(options: &PhpArray) -> io::Result<RootCertStore> {
    let Some(file) = string(options, "cafile") else {
        return Ok(phera_io_pool::tls::system_store().await);
    };
    let bytes = tokio::fs::read(PathBuf::from(file)).await?;
    let mut roots = RootCertStore::empty();
    for certificate in CertificateDer::pem_slice_iter(&bytes) {
        roots.add(certificate.map_err(invalid)?).map_err(invalid)?;
    }
    if roots.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "No certificates found in cafile",
        ));
    }
    Ok(roots)
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error.to_string())
}

#[derive(Debug)]
struct Verifier {
    roots: Option<RootCertStore>,
    verify_name: bool,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        certificate: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        _: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let certificate = ParsedCertificate::try_from(certificate)?;
        if let Some(roots) = &self.roots {
            verify_server_cert_signed_by_trust_anchor(
                &certificate,
                roots,
                intermediates,
                now,
                self.algorithms.all,
            )?;
        }
        if self.verify_name {
            verify_server_name(&certificate, name)?;
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(message, certificate, signature, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(message, certificate, signature, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}
