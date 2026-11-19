//! Exercise PDO TLS from a local TLS listener to the plain PostgreSQL CI service.

use std::sync::Arc;

use rustls::ServerConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

use super::Postgres;
use crate::drivers::tls::certificates::certificates;
use crate::dsn::Dsn;

struct Proxy {
    port: u16,
    task: JoinHandle<()>,
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Proxy {
    async fn start(config: Arc<ServerConfig>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let acceptor = TlsAcceptor::from(config.clone());
                tokio::spawn(async move {
                    let mut request = [0; 8];
                    if socket.read_exact(&mut request).await.is_err() {
                        return;
                    }
                    if request != [0, 0, 0, 8, 4, 210, 22, 47] {
                        // Reject the plaintext attempt in sslmode=allow. libpq and Phera retry.
                        let _ = socket
                            .write_all(b"E\0\0\0\x21SFATAL\0C28000\0MSSL required\0\0")
                            .await;
                        return;
                    }
                    if socket.write_all(b"S").await.is_err() {
                        return;
                    }
                    let Ok(mut encrypted) = acceptor.accept(socket).await else {
                        return;
                    };
                    let Ok(mut backend) = TcpStream::connect(("127.0.0.1", 54329)).await else {
                        return;
                    };
                    let _ = tokio::io::copy_bidirectional(&mut encrypted, &mut backend).await;
                });
            }
        });
        Self { port, task }
    }

    async fn connect(
        &self,
        host: &str,
        mode: &str,
        root: Option<&std::path::Path>,
        identity: Option<(&std::path::Path, &std::path::Path)>,
    ) -> Result<Postgres, crate::errors::DbError> {
        let mut dsn = format!(
            "pgsql:host={host};hostaddr=127.0.0.1;port={};\
            dbname=phera_test;sslmode={mode}",
            self.port
        );
        if let Some(root) = root {
            dsn.push_str(&format!(";sslrootcert={}", root.display()));
        }
        if let Some((cert, key)) = identity {
            dsn.push_str(&format!(
                ";sslcert={};sslkey={}",
                cert.display(),
                key.display()
            ));
        }
        Postgres::connect(
            &Dsn {
                driver: crate::dsn::DriverName::Postgres,
                fields: crate::dsn::postgres::fields(dsn.strip_prefix("pgsql:").unwrap(), true)
                    .unwrap(),
                options: Default::default(),
            },
            "phera",
            "phera",
        )
        .await
    }
}

#[tokio::test]
async fn verify_full_rejects_wrong_hostname_and_untrusted_chain() {
    let certs = certificates("localhost");
    let proxy = Proxy::start(certs.server(false)).await;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("root.pem");
    let unrelated_root = directory.path().join("unrelated.pem");
    tokio::fs::write(&root, certs.root).await.unwrap();
    tokio::fs::write(&unrelated_root, certificates("localhost").root)
        .await
        .unwrap();
    let wrong_host = proxy
        .connect("wrong.example", "verify-full", Some(&root), None)
        .await
        .err()
        .expect("verification must reject the name");
    assert!(
        wrong_host.detail.contains("does not match host name"),
        "{wrong_host:?}"
    );
    let untrusted = proxy
        .connect("localhost", "verify-full", Some(&unrelated_root), None)
        .await
        .err()
        .expect("verification must reject the chain");
    assert!(
        untrusted.detail.contains("certificate verify failed"),
        "{untrusted:?}"
    );
    let require = proxy
        .connect("localhost", "require", Some(&unrelated_root), None)
        .await
        .err()
        .expect("require with a CA must reject the chain");
    assert!(
        require.detail.contains("certificate verify failed"),
        "{require:?}"
    );
}

#[tokio::test]
async fn encrypted_queries_and_transaction_status_reach_the_database() {
    if TcpStream::connect(("127.0.0.1", 54329)).await.is_err() {
        eprintln!("SKIP: PostgreSQL test database unavailable");
        return;
    }
    let certs = certificates("localhost");
    let proxy = Proxy::start(certs.server(false)).await;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("root.pem");
    tokio::fs::write(&root, &certs.root).await.unwrap();
    for (host, mode, ca) in [
        ("localhost", "verify-full", Some(root.as_path())),
        ("wrong.example", "verify-ca", Some(root.as_path())),
        ("wrong.example", "require", Some(root.as_path())),
        ("wrong.example", "require", None),
        ("wrong.example", "prefer", None),
        ("wrong.example", "allow", None),
    ] {
        let driver = proxy.connect(host, mode, ca, None).await.unwrap();
        driver.run_unprepared("BEGIN").await.unwrap();
        assert!(driver.transaction_state().await, "{mode}");
        let row = driver
            .client
            .query_one("SELECT 42 AS answer", &[])
            .await
            .unwrap();
        assert_eq!(row.get::<_, i32>(0), 42, "{mode}");
        driver.run_unprepared("ROLLBACK").await.unwrap();
        assert!(!driver.transaction_state().await, "{mode}");
    }
}

#[tokio::test]
async fn encrypted_connections_present_the_configured_client_certificate() {
    if TcpStream::connect(("127.0.0.1", 54329)).await.is_err() {
        eprintln!("SKIP: PostgreSQL test database unavailable");
        return;
    }
    let certs = certificates("localhost");
    let proxy = Proxy::start(certs.server(true)).await;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("root.pem");
    let cert = directory.path().join("client.pem");
    let key = directory.path().join("client.key");
    tokio::fs::write(&root, certs.root).await.unwrap();
    tokio::fs::write(&cert, certs.client).await.unwrap();
    let mut options = tokio::fs::OpenOptions::new();
    let mut key_file = options
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&key)
        .await
        .unwrap();
    key_file
        .write_all(certs.client_key.as_bytes())
        .await
        .unwrap();
    drop(key_file);

    assert!(
        proxy
            .connect("localhost", "verify-full", Some(&root), None)
            .await
            .is_err(),
    );
    proxy
        .connect("localhost", "verify-full", Some(&root), Some((&cert, &key)))
        .await
        .unwrap();
}
