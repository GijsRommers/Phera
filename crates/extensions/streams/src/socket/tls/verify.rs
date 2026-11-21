//! `verify_peer`, `verify_depth`, and `peer_fingerprint` constrain which peers connect.

use rustls::{
    DigitallySignedStruct, Error, RootCertStore, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use sha2::Digest;
use webpki::{EndEntityCert, KeyUsage, VerifiedPath};

use super::Options;

#[derive(Debug)]
pub(super) struct Verifier {
    roots: Option<RootCertStore>,
    verify_name: bool,
    allow_self_signed: bool,
    depth: Option<usize>,
    pins: Vec<Pin>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl Verifier {
    pub fn new(
        options: &Options,
        roots: RootCertStore,
        algorithms: WebPkiSupportedAlgorithms,
    ) -> std::io::Result<Self> {
        let mut pins = Vec::new();
        for (algorithm, hex) in &options.fingerprints {
            pins.push(Pin::new(algorithm.as_deref(), hex)?);
        }
        Ok(Self {
            roots: options.verify_peer.then_some(roots),
            verify_name: options.verify_name,
            allow_self_signed: options.allow_self_signed,
            depth: options.verify_depth,
            pins,
            algorithms,
        })
    }
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
        let leaf = EndEntityCert::try_from(certificate).map_err(certificate_error)?;
        let depth = |path: &VerifiedPath<'_>| {
            if self
                .depth
                .is_some_and(|max| path.intermediate_certificates().count() > max)
            {
                return Err(webpki::Error::MaximumPathDepthExceeded);
            }
            Ok(())
        };
        if let Some(roots) = &self.roots {
            let verify = |roots: &RootCertStore, chain| {
                leaf.verify_for_usage(
                    self.algorithms.all,
                    &roots.roots,
                    chain,
                    now,
                    KeyUsage::server_auth(),
                    None,
                    Some(&depth),
                )
                .map(|_| ())
            };
            let mut result = verify(roots, intermediates);
            if result == Err(webpki::Error::UnknownIssuer) && self.allow_self_signed {
                let mut explicit = RootCertStore::empty();
                explicit.add(certificate.clone().into_owned())?;
                result = verify(&explicit, &[]);
            }
            result.map_err(certificate_error)?;
        }
        if self.verify_name {
            leaf.verify_is_valid_for_subject_name(name)
                .map_err(certificate_error)?;
        }
        if self.pins.iter().any(|pin| !pin.matches(certificate)) {
            return Err(Error::General("peer_fingerprint match failure".into()));
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

fn certificate_error(error: webpki::Error) -> Error {
    Error::General(format!("certificate verify failed: {error}"))
}

#[derive(Debug)]
struct Pin {
    hash: Hash,
    bytes: Vec<u8>,
}

#[derive(Debug)]
enum Hash {
    Md5,
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
}

impl Pin {
    fn new(hash: Option<&[u8]>, hex: &[u8]) -> std::io::Result<Self> {
        let hash = match (
            hash.map(|name| name.to_ascii_lowercase()).as_deref(),
            hex.len(),
        ) {
            (None | Some(b"md5"), 32) => Hash::Md5,
            (None | Some(b"sha1"), 40) => Hash::Sha1,
            (Some(b"sha224"), 56) => Hash::Sha224,
            (Some(b"sha256"), 64) => Hash::Sha256,
            (Some(b"sha384"), 96) => Hash::Sha384,
            (Some(b"sha512"), 128) => Hash::Sha512,
            _ => return Err(super::invalid("Invalid SSL peer fingerprint")),
        };
        let bytes = hex
            .chunks_exact(2)
            .map(|pair| {
                let high = char::from(pair[0]).to_digit(16)?;
                let low = char::from(pair[1]).to_digit(16)?;
                Some((high * 16 + low) as u8)
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| super::invalid("Invalid SSL peer fingerprint"))?;
        Ok(Self { hash, bytes })
    }

    fn matches(&self, certificate: &[u8]) -> bool {
        match self.hash {
            Hash::Md5 => md5::Md5::digest(certificate)[..] == self.bytes,
            Hash::Sha1 => sha1::Sha1::digest(certificate)[..] == self.bytes,
            Hash::Sha224 => sha2::Sha224::digest(certificate)[..] == self.bytes,
            Hash::Sha256 => sha2::Sha256::digest(certificate)[..] == self.bytes,
            Hash::Sha384 => sha2::Sha384::digest(certificate)[..] == self.bytes,
            Hash::Sha512 => sha2::Sha512::digest(certificate)[..] == self.bytes,
        }
    }
}
