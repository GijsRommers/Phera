//! A local HTTPS release upgrades a disposable copy of the CLI.

use std::convert::Infallible;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;

use flate2::{Compression, write::GzEncoder};
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Response, StatusCode, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::{ServerConfig, pki_types::PrivatePkcs8KeyDer};

#[tokio::test]
async fn check_and_upgrade_a_copy_over_https() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("phera");
    let old = directory.path().join("phera.old");
    let installed = env!("CARGO_PKG_VERSION");
    let latest = semver::Version::parse(installed).unwrap().major + 1;
    let latest = format!("{latest}.0.0");
    let previous = "0.0.0";
    fs::copy(env!("CARGO_BIN_EXE_phera"), &executable).unwrap();
    fs::write(&old, "previous backup").unwrap();
    let original = fs::metadata(&executable).unwrap().ino();

    let ca = directory.path().join("ca.pem");
    let (base, server) = release_server(&ca, &latest, previous).await;

    let checked = run(&executable, &base, &ca, &["upgrade", "--check"]).await;
    assert!(checked.status.success(), "{checked:?}");
    let report = String::from_utf8(checked.stdout).unwrap();
    assert!(
        report.contains(installed) && report.contains(&latest),
        "{report}"
    );
    assert_eq!(fs::metadata(&executable).unwrap().ino(), original);

    fs::set_permissions(&executable, fs::Permissions::from_mode(0o555)).unwrap();
    let refused = run(&executable, &base, &ca, &["upgrade"]).await;
    assert!(!refused.status.success(), "{refused:?}");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("cannot write"));
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();

    let invalid = run(
        &executable,
        &base,
        &ca,
        &["upgrade", "--version", "999.0.0"],
    )
    .await;
    assert!(!invalid.status.success(), "{invalid:?}");
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("SHA-256 verification failed"));
    assert_eq!(fs::metadata(&executable).unwrap().ino(), original);
    assert_eq!(fs::read_to_string(&old).unwrap(), "previous backup");

    let upgraded = run(&executable, &base, &ca, &["upgrade"]).await;
    assert!(upgraded.status.success(), "{upgraded:?}");
    let report = String::from_utf8(upgraded.stdout).unwrap();
    assert!(
        report.contains(installed) && report.contains(&latest),
        "{report}"
    );
    assert_eq!(fs::metadata(&old).unwrap().ino(), original);
    let version = run(&executable, &base, &ca, &["--version"]).await;
    assert!(version.status.success(), "{version:?}");
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        format!("Phera {latest}")
    );

    let downgrade_dir = directory.path().join("downgrade");
    fs::create_dir(&downgrade_dir).unwrap();
    let executable = downgrade_dir.join("phera");
    fs::copy(&old, &executable).unwrap();
    let downgraded = run(&executable, &base, &ca, &["upgrade", "--version", previous]).await;
    assert!(downgraded.status.success(), "{downgraded:?}");
    let version = run(&executable, &base, &ca, &["--version"]).await;
    assert!(version.status.success(), "{version:?}");
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        format!("Phera {previous}")
    );
    server.abort();
}

async fn run(executable: &Path, base: &str, ca: &Path, arguments: &[&str]) -> Output {
    let executable = executable.to_owned();
    let base = base.to_owned();
    let ca = ca.to_owned();
    let arguments: Vec<String> = arguments.iter().map(ToString::to_string).collect();
    tokio::task::spawn_blocking(move || {
        let mut command = Command::new(executable);
        command.env_clear();
        for name in ["PATH", "HOME", "TMPDIR"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command
            .env("PHERA_RELEASE_BASE_URL", base)
            .env("SSL_CERT_FILE", ca)
            .args(arguments)
            .output()
            .unwrap()
    })
    .await
    .unwrap()
}

async fn release_server(
    ca: &Path,
    latest: &str,
    previous: &str,
) -> (String, tokio::task::JoinHandle<()>) {
    let cert = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).unwrap();
    fs::write(ca, cert.cert.pem()).unwrap();
    let tls = ServerConfig::builder_with_provider(phera_io_pool::tls::provider().clone())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.cert.der().clone()],
            PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()).into(),
        )
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(tls));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("https://{}", listener.local_addr().unwrap());
    let mut assets = std::collections::HashMap::new();
    assets.insert(
        "/latest".to_owned(),
        Bytes::from(format!(r#"{{"tag_name":"v{latest}"}}"#)),
    );
    for (version, corrupt) in [(latest, false), (previous, false), ("999.0.0", true)] {
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "macos-arm64",
            ("macos", "x86_64") => "macos-x86_64",
            ("linux", "aarch64") => "linux-arm64",
            ("linux", "x86_64") => "linux-x86_64",
            _ => unreachable!("unsupported test platform"),
        };
        let name = format!("phera-{version}-{platform}.tar.gz");
        let archive = archive(version);
        let digest = if corrupt {
            "0".repeat(64)
        } else {
            format!("{:x}", Sha256::digest(&archive))
        };
        assets.insert(
            format!("/download/v{version}/SHA256SUMS"),
            Bytes::from(format!("{digest}  {name}\n")),
        );
        assets.insert(format!("/download/v{version}/{name}"), Bytes::from(archive));
    }
    let assets = Arc::new(assets);
    let server = tokio::spawn(async move {
        loop {
            let (tcp, _) = listener.accept().await.unwrap();
            let acceptor = acceptor.clone();
            let assets = assets.clone();
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let service = service_fn(move |request: hyper::Request<_>| {
                    let response = match assets.get(request.uri().path()) {
                        Some(bytes) => Response::new(Full::new(bytes.clone())),
                        None => Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Full::new(Bytes::new()))
                            .unwrap(),
                    };
                    async { Ok::<_, Infallible>(response) }
                });
                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(tls), service)
                    .await;
            });
        }
    });
    (base, server)
}

fn archive(version: &str) -> Vec<u8> {
    let binary = format!("#!/bin/sh\nprintf 'Phera {version}\\n'\n");
    let mut header = tar::Header::new_ustar();
    header.set_mode(0o755);
    header.set_size(binary.len() as u64);
    let mut archive = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    archive
        .append_data(&mut header, "phera", binary.as_bytes())
        .unwrap();
    archive.into_inner().unwrap().finish().unwrap()
}
