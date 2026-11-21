//! `cafile`, `capath`, and `local_cert` load TLS material without blocking the worker.

use std::{io, path::Path};

use pkcs8::{
    EncryptedPrivateKeyInfoRef,
    der::{Decode, SecretDocument},
};
use rustls::{
    RootCertStore,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, pem::PemObject},
};
use sha2::{Digest, Sha256};

use super::{Options, invalid};

pub(in crate::socket) struct Material {
    pub roots: RootCertStore,
    pub identity: Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>,
    pub stamp: [u8; 32],
}

impl Material {
    pub async fn load(options: &Options) -> io::Result<Self> {
        let mut stamp = Sha256::new();
        let mut roots = RootCertStore::empty();
        if options.verify_peer {
            if let Some(file) = &options.cafile {
                let pem = read(file, &mut stamp).await?;
                let certificates = certificates(&pem)?;
                for certificate in certificates {
                    roots.add(certificate).map_err(invalid)?;
                }
            }
            if let Some(path) = &options.capath {
                directory(path, &mut stamp, &mut roots).await?;
            }
            if options.cafile.is_none() && options.capath.is_none() {
                roots = phera_io_pool::tls::system_store().await;
            }
        }
        let identity = if let Some(path) = &options.local_cert {
            let pem = read(path, &mut stamp).await?;
            let leaf = certificates(&pem)?;
            let key = if let Some(path) = &options.local_pk {
                let bytes = read(path, &mut stamp).await?;
                private_key(bytes, options.passphrase.as_deref()).await?
            } else {
                private_key(pem, options.passphrase.as_deref()).await?
            };
            Some((leaf, key))
        } else {
            None
        };
        Ok(Self {
            roots,
            identity,
            stamp: stamp.finalize().into(),
        })
    }
}

async fn read(file: &Path, stamp: &mut Sha256) -> io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;

    const MAX_PEM: u64 = 16 * 1024 * 1024;
    let mut bytes = Vec::new();
    tokio::fs::File::open(file)
        .await?
        .take(MAX_PEM + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() as u64 > MAX_PEM {
        return Err(invalid("TLS certificate or key file is too large"));
    }
    stamp.update((bytes.len() as u64).to_le_bytes());
    stamp.update(&bytes);
    Ok(bytes)
}

async fn directory(path: &Path, stamp: &mut Sha256, roots: &mut RootCertStore) -> io::Result<()> {
    let mut entries = tokio::fs::read_dir(path).await?;
    let mut paths = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let leaf = entry.file_name();
        let Some((hash, suffix)) = leaf.to_str().and_then(|leaf| leaf.split_once('.')) else {
            continue;
        };
        if hash.len() == 8
            && hash.bytes().all(|b| b.is_ascii_hexdigit())
            && !suffix.is_empty()
            && suffix.bytes().all(|b| b.is_ascii_digit())
        {
            paths.push(entry.path());
        }
    }
    paths.sort_unstable();
    for path in paths {
        let bytes = read(&path, stamp).await?;
        for certificate in certificates(&bytes)? {
            roots.add(certificate).map_err(invalid)?;
        }
    }
    Ok(())
}

fn certificates(bytes: &[u8]) -> io::Result<Vec<CertificateDer<'static>>> {
    let certificates = CertificateDer::pem_slice_iter(bytes)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if certificates.is_empty() {
        Err(invalid("No certificates found in TLS PEM file"))
    } else {
        Ok(certificates)
    }
}

async fn private_key(bytes: Vec<u8>, password: Option<&str>) -> io::Result<PrivateKeyDer<'static>> {
    if let Ok(key) = PrivateKeyDer::from_pem_slice(&bytes) {
        return Ok(key);
    }
    let password = password.unwrap_or_default().to_owned();
    tokio::task::spawn_blocking(move || encrypted_key(&bytes, &password))
        .await
        .map_err(invalid)?
}

fn encrypted_key(bytes: &[u8], password: &str) -> io::Result<PrivateKeyDer<'static>> {
    let marker = b"-----BEGIN ENCRYPTED PRIVATE KEY-----";
    let start = bytes
        .windows(marker.len())
        .position(|window| window == marker)
        .ok_or_else(|| invalid("No private key found in TLS PEM file"))?;
    let pem = std::str::from_utf8(&bytes[start..]).map_err(invalid)?;
    let (_, document) = SecretDocument::from_pem(pem).map_err(invalid)?;
    let info = EncryptedPrivateKeyInfoRef::from_der(document.as_bytes()).map_err(invalid)?;
    let key = info.decrypt(password.as_bytes()).map_err(invalid)?;
    Ok(PrivatePkcs8KeyDer::from(key.as_bytes().to_vec()).into())
}
