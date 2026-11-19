//! Parse the PEM certificates selected by PostgreSQL's `sslrootcert` and `sslcert`.

use rustls::pki_types::{CertificateDer, pem::PemObject};

#[cfg(test)]
pub mod certificates;

pub fn certificates(
    bytes: &[u8],
) -> Result<Vec<CertificateDer<'static>>, rustls::pki_types::pem::Error> {
    CertificateDer::pem_slice_iter(bytes).collect()
}
