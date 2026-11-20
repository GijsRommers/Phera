//! Builds local server and client certificate material for the same PHP fixtures.

use std::{fs, path::Path, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD};
use pkcs8::{LineEnding, PrivateKeyInfoRef, der::Decode, pkcs5::pbes2};
use rcgen::{BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair};
use rustls::{
    RootCertStore, ServerConfig,
    pki_types::PrivatePkcs8KeyDer,
    server::{ParsedCertificate, WebPkiClientVerifier},
};
use sha2::{Digest, Sha256};

pub(super) fn config(directory: &Path, ca: &Path) -> (ServerConfig, String) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    fs::write(ca, cert.cert.pem()).unwrap();
    let capath = directory.join("capath");
    fs::create_dir(&capath).unwrap();
    // OpenSSL's subject hash for rcgen's default distinguished name.
    fs::copy(ca, capath.join("3b291c1d.0")).unwrap();
    let public_key = ParsedCertificate::try_from(cert.cert.der())
        .unwrap()
        .subject_public_key_info();
    let pin = format!("sha256//{}", STANDARD.encode(Sha256::digest(public_key)));

    let provider = phera_io_pool::tls::provider().clone();
    let verifier = WebPkiClientVerifier::builder_with_provider(
        Arc::new(client_material(directory)),
        Arc::clone(&provider),
    )
    .allow_unauthenticated()
    .build()
    .unwrap();
    let mut tls = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_client_cert_verifier(verifier)
        .with_single_cert(
            vec![cert.cert.der().clone()],
            PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()).into(),
        )
        .unwrap();
    tls.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    (tls, pin)
}

fn client_material(directory: &Path) -> RootCertStore {
    let mut ca = CertificateParams::default();
    ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let issuer = rcgen::CertifiedIssuer::self_signed(ca, KeyPair::generate().unwrap()).unwrap();
    let mut params = CertificateParams::default();
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    let key = KeyPair::generate().unwrap();
    let certificate = params.signed_by(&key, &issuer).unwrap();
    fs::write(directory.join("client.pem"), certificate.pem()).unwrap();
    fs::write(directory.join("client.der"), certificate.der()).unwrap();
    fs::write(directory.join("client-key.der"), key.serialized_der()).unwrap();
    let parameters =
        pbes2::Parameters::generate_pbkdf2_sha256_aes256cbc(2048, &[1; 16], [2; 16]).unwrap();
    let encrypted = PrivateKeyInfoRef::from_der(key.serialized_der())
        .unwrap()
        .encrypt_with_params(parameters, b"secret")
        .unwrap();
    let pem = encrypted
        .to_pem("ENCRYPTED PRIVATE KEY", LineEnding::LF)
        .unwrap();
    fs::write(directory.join("client-key.pem"), pem.as_bytes()).unwrap();
    let mut roots = RootCertStore::empty();
    roots.add(issuer.der().clone()).unwrap();
    roots
}
