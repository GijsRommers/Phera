//! Trust roots for `sslrootcert=system` and MySQL TLS connections.

use std::sync::OnceLock;

use rustls::RootCertStore;
use rustls::pki_types::{CertificateDer, pem::PemObject};

pub async fn platform_roots() -> &'static [CertificateDer<'static>] {
    static CERTIFICATES: OnceLock<Vec<CertificateDer<'static>>> = OnceLock::new();
    if let Some(certificates) = CERTIFICATES.get() {
        return certificates;
    }
    tokio::task::spawn_blocking(|| {
        CERTIFICATES.get_or_init(|| rustls_native_certs::load_native_certs().certs)
    })
    .await
    .map_or(&[], Vec::as_slice)
}

pub async fn system_store() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(platform_roots().await.iter().cloned());
    if roots.is_empty() {
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }
    roots
}

pub fn certificates(
    bytes: &[u8],
) -> Result<Vec<CertificateDer<'static>>, rustls::pki_types::pem::Error> {
    CertificateDer::pem_slice_iter(bytes).collect()
}
