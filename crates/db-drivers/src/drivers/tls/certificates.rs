//! Certificates for local PostgreSQL and MySQL TLS tests.

use std::sync::Arc;

use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};

pub struct TestCertificates {
    pub root: String,
    root_der: CertificateDer<'static>,
    server_der: CertificateDer<'static>,
    server_key: PrivateKeyDer<'static>,
    pub client: String,
    pub client_key: String,
}

pub fn certificates(host: &str) -> TestCertificates {
    let ca_key = KeyPair::generate().unwrap();
    let mut ca = CertificateParams::new(Vec::new()).unwrap();
    ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let root = ca.self_signed(&ca_key).unwrap();
    let issuer = Issuer::from_params(&ca, &ca_key);
    let leaf_key = KeyPair::generate().unwrap();
    let leaf = CertificateParams::new(vec![host.into()])
        .unwrap()
        .signed_by(&leaf_key, &issuer)
        .unwrap();
    let client_key = KeyPair::generate().unwrap();
    let client = CertificateParams::default()
        .signed_by(&client_key, &issuer)
        .unwrap();
    TestCertificates {
        root: root.pem(),
        root_der: root.der().clone(),
        server_der: leaf.der().clone(),
        server_key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der())),
        client: client.pem(),
        client_key: client_key.serialize_pem(),
    }
}

impl TestCertificates {
    pub fn server(&self, require_client: bool) -> Arc<ServerConfig> {
        let provider = phera_io_pool::tls::provider().clone();
        let builder = ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap();
        let builder = if require_client {
            let mut store = RootCertStore::empty();
            store.add(self.root_der.clone()).unwrap();
            let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(store), provider)
                .build()
                .unwrap();
            builder.with_client_cert_verifier(verifier)
        } else {
            builder.with_no_client_auth()
        };
        Arc::new(
            builder
                .with_single_cert(vec![self.server_der.clone()], self.server_key.clone_key())
                .unwrap(),
        )
    }
}
