//! `Pdo\Mysql::ATTR_SSL_CA` selects trusted roots for a MySQL TLS connection.

use std::path::PathBuf;
use std::sync::Once;

use mysql_async::{ClientIdentity, SslOpts};

use crate::connect_options::MysqlTls;
use crate::drivers::tls;
use crate::errors::DbError;

pub(super) async fn options(settings: &MysqlTls) -> Result<SslOpts, DbError> {
    static PROVIDER: Once = Once::new();
    PROVIDER.call_once(|| {
        let _ = rustls_rustcrypto::provider().install_default();
    });

    let custom_roots = settings.ca.is_some() || settings.capath.is_some();
    let mut roots = Vec::new();
    if let Some(file) = &settings.ca {
        roots.push(PathBuf::from(file).into());
    }
    if let Some(path) = &settings.capath {
        let mut entries = tokio::fs::read_dir(path).await.map_err(|_| failure())?;
        while let Some(entry) = entries.next_entry().await.map_err(|_| failure())? {
            let path = entry.path();
            let extension = path.extension().and_then(|suffix| suffix.to_str());
            if extension.is_some_and(|suffix| {
                matches!(suffix, "pem" | "crt" | "cer" | "der")
                    || suffix.bytes().all(|byte| byte.is_ascii_digit())
            }) && tokio::fs::metadata(&path)
                .await
                .is_ok_and(|metadata| metadata.is_file())
            {
                roots.push(path.into());
            }
        }
    }
    if !custom_roots {
        let mut store = rustls::RootCertStore::empty();
        for cert in tls::platform_roots().await {
            if store.add(cert.clone()).is_ok() {
                roots.push(cert.to_vec().into());
            }
        }
    }
    if custom_roots && roots.is_empty() {
        return Err(failure());
    }
    let identity = match (&settings.cert, &settings.key) {
        (Some(cert), Some(key)) => Some(ClientIdentity::new(
            PathBuf::from(cert).into(),
            PathBuf::from(key).into(),
        )),
        (None, None) => None,
        _ => return Err(failure()),
    };
    let disable_builtin = custom_roots || !roots.is_empty();
    Ok(SslOpts::default()
        .with_root_certs(roots)
        .with_disable_built_in_roots(disable_builtin)
        .with_client_identity(identity)
        .with_danger_accept_invalid_certs(!settings.verify))
}

pub(super) fn failure() -> DbError {
    DbError {
        state: "HY000".into(),
        code: Some(2002),
        detail: "Cannot connect to MySQL using SSL".into(),
    }
}
