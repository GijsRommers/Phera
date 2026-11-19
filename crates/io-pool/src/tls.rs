//! TLS policy for pooled connections such as `$redis->connect('tls://redis.example')`.

use std::sync::{Arc, OnceLock};

use rustls::crypto::CryptoProvider;
use rustls::pki_types::CertificateDer;
use rustls::{ClientConfig, ConfigBuilder, RootCertStore, WantsVerifier};

pub fn provider() -> &'static Arc<CryptoProvider> {
    static PROVIDER: OnceLock<Arc<CryptoProvider>> = OnceLock::new();
    PROVIDER.get_or_init(|| Arc::new(rustls::crypto::ring::default_provider()))
}

/// Ring's default suites over TLS 1.2 and TLS 1.3.
pub fn client_builder() -> Result<ConfigBuilder<ClientConfig, WantsVerifier>, rustls::Error> {
    use rustls::version::{TLS12, TLS13};
    ClientConfig::builder_with_provider(provider().clone())
        .with_protocol_versions(&[&TLS13, &TLS12])
}

/// Redis uses the bundled roots and authenticates the server by host name.
pub fn webpki_client() -> Result<Arc<ClientConfig>, rustls::Error> {
    static CONFIG: OnceLock<Result<Arc<ClientConfig>, rustls::Error>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            Ok(Arc::new(
                client_builder()?
                    .with_root_certificates(webpki_store())
                    .with_no_client_auth(),
            ))
        })
        .clone()
}

fn webpki_store() -> RootCertStore {
    RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    }
}

/// Keep only roots that can be added to rustls. An empty list selects WebPKI.
pub async fn platform_roots() -> &'static [CertificateDer<'static>] {
    static CERTIFICATES: OnceLock<Vec<CertificateDer<'static>>> = OnceLock::new();
    if let Some(certificates) = CERTIFICATES.get() {
        return certificates;
    }
    tokio::task::spawn_blocking(|| {
        CERTIFICATES.get_or_init(|| {
            let mut store = RootCertStore::empty();
            rustls_native_certs::load_native_certs()
                .certs
                .into_iter()
                .filter(|cert| store.add(cert.clone()).is_ok())
                .collect()
        })
    })
    .await
    .map_or(&[], Vec::as_slice)
}

pub async fn system_store() -> RootCertStore {
    let certificates = platform_roots().await;
    if certificates.is_empty() {
        return webpki_store();
    }
    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(certificates.iter().cloned());
    roots
}
