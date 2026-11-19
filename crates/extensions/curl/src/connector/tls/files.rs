//! Loads explicit CA sources and optional client authentication off the worker.

use std::{fs, path::Path};

use pkcs8::{
    EncryptedPrivateKeyInfoRef,
    der::{Decode, SecretDocument},
};
use rustls::{
    RootCertStore,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, pem::PemObject},
};

use crate::{
    error::TransferError,
    options::{TlsFiles, TlsFormat},
};

pub(super) type Identity = (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>);

pub(super) fn load(
    files: &TlsFiles,
    verify_peer: bool,
    roots: RootCertStore,
) -> Result<(RootCertStore, Option<Identity>), TransferError> {
    let roots = if verify_peer {
        load_roots(roots, files)?
    } else {
        roots
    };
    let identity = identity(files)?;
    Ok((roots, identity))
}

fn load_roots(initial: RootCertStore, files: &TlsFiles) -> Result<RootCertStore, TransferError> {
    let mut roots = if let Some(bytes) = &files.ca_blob {
        let mut roots = RootCertStore::empty();
        if !add_roots(&mut roots, bytes) {
            return Err(TransferError::new(
                77,
                "error importing CA certificate blob",
            ));
        }
        roots
    } else if let Some(path) = &files.ca_file {
        let mut roots = RootCertStore::empty();
        let invalid = || {
            TransferError::new(
                77,
                format!("error setting certificate file: {}", path.display()),
            )
        };
        let pem = fs::read(path).map_err(|_| invalid())?;
        if !add_roots(&mut roots, &pem) {
            return Err(invalid());
        }
        roots
    } else {
        initial
    };
    if let Some(path) = &files.ca_dir {
        let invalid = || {
            TransferError::new(
                77,
                format!("error setting certificate path: {}", path.display()),
            )
        };
        for entry in fs::read_dir(path).map_err(|_| invalid())? {
            let entry = entry.map_err(|_| invalid())?;
            let name = entry.file_name();
            let Some((hash, index)) = name.to_str().and_then(|name| name.split_once('.')) else {
                continue;
            };
            if hash.len() == 8
                && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                && !index.is_empty()
                && index.bytes().all(|byte| byte.is_ascii_digit())
            {
                let pem = fs::read(entry.path()).map_err(|_| invalid())?;
                add_roots(&mut roots, &pem);
            }
        }
    }
    Ok(roots)
}

fn add_roots(roots: &mut RootCertStore, bytes: &[u8]) -> bool {
    let (valid, _) = roots
        .add_parsable_certificates(CertificateDer::pem_slice_iter(bytes).filter_map(Result::ok));
    valid > 0
}

fn identity(files: &TlsFiles) -> Result<Option<Identity>, TransferError> {
    if files.cert.is_none() && files.cert_blob.is_none() {
        return Ok(None);
    }
    let invalid_cert = || TransferError::new(58, "could not load client certificate");
    let invalid_key = || TransferError::new(58, "could not load private key");
    let certificate =
        read(files.cert_blob.as_deref(), files.cert.as_deref()).ok_or_else(invalid_cert)?;
    let chain = match files.cert_type {
        TlsFormat::Der => vec![CertificateDer::from(certificate.clone())],
        TlsFormat::Pem => CertificateDer::pem_slice_iter(&certificate)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| invalid_cert())?,
    };
    if chain.is_empty() {
        return Err(invalid_cert());
    }
    let password = files.key_password.as_deref().unwrap_or_default();
    let key = if files.key.is_some() || files.key_blob.is_some() {
        let bytes =
            read(files.key_blob.as_deref(), files.key.as_deref()).ok_or_else(invalid_key)?;
        private_key(&bytes, files.key_type, password)
    } else {
        private_key(&certificate, files.key_type, password)
    }
    .ok_or_else(invalid_key)?;
    Ok(Some((chain, key)))
}

fn read(blob: Option<&[u8]>, path: Option<&Path>) -> Option<Vec<u8>> {
    if let Some(blob) = blob {
        Some(blob.to_vec())
    } else {
        fs::read(path?).ok()
    }
}

fn private_key(bytes: &[u8], format: TlsFormat, password: &[u8]) -> Option<PrivateKeyDer<'static>> {
    match format {
        TlsFormat::Pem => PrivateKeyDer::from_pem_slice(bytes).ok().or_else(|| {
            let document = encrypted_pem(bytes)?;
            decrypt(document.as_bytes(), password)
        }),
        TlsFormat::Der => PrivateKeyDer::try_from(bytes.to_vec())
            .ok()
            .or_else(|| decrypt(bytes, password)),
    }
}

fn encrypted_pem(bytes: &[u8]) -> Option<SecretDocument> {
    let pem = std::str::from_utf8(bytes).ok()?;
    let start = pem.find("-----BEGIN ENCRYPTED PRIVATE KEY-----")?;
    let end = "-----END ENCRYPTED PRIVATE KEY-----";
    let finish = start + pem[start..].find(end)? + end.len();
    let block = pem.get(start..finish)?;
    let (_, document) = SecretDocument::from_pem(block).ok()?;
    Some(document)
}

fn decrypt(bytes: &[u8], password: &[u8]) -> Option<PrivateKeyDer<'static>> {
    let encrypted = EncryptedPrivateKeyInfoRef::from_der(bytes).ok()?;
    let decrypted = encrypted.decrypt(password).ok()?;
    Some(PrivatePkcs8KeyDer::from(decrypted.as_bytes().to_vec()).into())
}
