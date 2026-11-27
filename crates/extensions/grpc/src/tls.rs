//! TLS for `https://` channels and for a server given a certificate, with
//! the rustls settings Phera's other clients use. HTTP/2 is chosen by ALPN.

use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::version::{TLS12, TLS13};
use rustls::{ClientConfig, ServerConfig};

const H2: &[u8] = b"h2";

/// A server configuration from PEM files: the certificate chain and its key.
pub fn server_config(certificate: &str, key: &str) -> Result<Arc<ServerConfig>, String> {
    let chain = CertificateDer::pem_file_iter(certificate)
        .and_then(|certificates| certificates.collect::<Result<Vec<_>, _>>())
        .map_err(|error| format!("cannot read certificate {certificate}: {error}"))?;
    if chain.is_empty() {
        return Err(format!("no certificate in {certificate}"));
    }
    let key = PrivateKeyDer::from_pem_file(key)
        .map_err(|error| format!("cannot read private key {key}: {error}"))?;
    let mut config = ServerConfig::builder_with_provider(phera_io_pool::tls::provider().clone())
        .with_protocol_versions(&[&TLS13, &TLS12])
        .map_err(|error| error.to_string())?
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .map_err(|error| format!("invalid certificate or key: {error}"))?;
    config.alpn_protocols = vec![H2.to_vec()];
    Ok(Arc::new(config))
}

/// A client configuration trusting the system's roots and, when given, the
/// certificates in a PEM file (a private CA, a self-signed server).
pub async fn client_config(ca_file: Option<&str>) -> Result<Arc<ClientConfig>, String> {
    let mut roots = phera_io_pool::tls::system_store().await;
    if let Some(ca_file) = ca_file {
        let certificates = CertificateDer::pem_file_iter(ca_file)
            .and_then(|certificates| certificates.collect::<Result<Vec<_>, _>>())
            .map_err(|error| format!("cannot read CA file {ca_file}: {error}"))?;
        for certificate in certificates {
            roots
                .add(certificate)
                .map_err(|error| format!("invalid certificate in {ca_file}: {error}"))?;
        }
    }
    let mut config = phera_io_pool::tls::client_builder()
        .map_err(|error| error.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![H2.to_vec()];
    Ok(Arc::new(config))
}
