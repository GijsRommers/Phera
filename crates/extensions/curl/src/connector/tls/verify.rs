//! The TLS signature is always verified, including with explicit trust overrides.

use std::{fmt, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD};
use rustls::{
    DigitallySignedStruct, Error, OtherError, RootCertStore, SignatureScheme,
    client::{
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        verify_server_cert_signed_by_trust_anchor, verify_server_name,
    },
    crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature},
    pki_types::{CertificateDer, ServerName, UnixTime},
    server::ParsedCertificate,
};
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub(super) struct Verifier {
    pub(super) roots: Option<RootCertStore>,
    pub(super) verify_host: bool,
    pub(super) pins: Option<Arc<str>>,
    pub(super) algorithms: WebPkiSupportedAlgorithms,
}

#[derive(Debug)]
pub(super) struct PinMismatch;

impl fmt::Display for PinMismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SSL: public key does not match pinned public key")
    }
}

impl std::error::Error for PinMismatch {}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        certificate: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        _ocsp: &[u8],
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
        if self.verify_host {
            verify_server_name(&certificate, name)?;
        }
        if let Some(pins) = &self.pins {
            let public_key = certificate.subject_public_key_info();
            let hash = STANDARD.encode(Sha256::digest(public_key.as_ref()));
            if !pins
                .split(';')
                .any(|pin| pin.strip_prefix("sha256//") == Some(hash.as_str()))
            {
                return Err(OtherError(Arc::new(PinMismatch)).into());
            }
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
