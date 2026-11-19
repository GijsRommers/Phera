//! SHA-256 pins are checked after rustls validates the server certificate.

use std::{fmt, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD};
use rustls::{
    DigitallySignedStruct, Error, OtherError, SignatureScheme,
    client::{
        WebPkiServerVerifier,
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    },
    pki_types::{CertificateDer, ServerName, UnixTime},
    server::ParsedCertificate,
};
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub(super) struct Pinned {
    pub(super) verifier: Arc<WebPkiServerVerifier>,
    pub(super) pins: Arc<str>,
}

#[derive(Debug)]
pub(super) struct PinMismatch;

impl fmt::Display for PinMismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SSL: public key does not match pinned public key")
    }
}

impl std::error::Error for PinMismatch {}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        certificate: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let verified =
            self.verifier
                .verify_server_cert(certificate, intermediates, name, ocsp, now)?;
        let public_key = ParsedCertificate::try_from(certificate)?.subject_public_key_info();
        let hash = STANDARD.encode(Sha256::digest(public_key.as_ref()));
        if !self
            .pins
            .split(';')
            .any(|pin| pin.strip_prefix("sha256//") == Some(hash.as_str()))
        {
            return Err(OtherError(Arc::new(PinMismatch)).into());
        }
        Ok(verified)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.verifier
            .verify_tls12_signature(message, certificate, signature)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.verifier
            .verify_tls13_signature(message, certificate, signature)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.verifier.supported_verify_schemes()
    }
}
