//! Exercise the full caching_sha2_password exchange without a warm MySQL auth cache.

use std::sync::Arc;

use mysql_async::{Conn, OptsBuilder};
use rsa::pkcs8::{EncodePublicKey, LineEnding};
use rsa::{Generate, Oaep, PrivateKey};
use rustls::ServerConfig;
use sha1_011::Sha1;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use super::tls;
use crate::connect_options::MysqlTls;
use crate::drivers::tls::certificates::certificates;

const PASSWORD: &str = "pdo-test-password";
const NONCE: &[u8; 20] = b"test-nonce-123456789";

#[tokio::test]
async fn caching_sha2_password_uses_verified_tls_or_rsa() {
    let (pem, config) = certificates("127.0.0.1");
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("root.pem");
    tokio::fs::write(&root, pem).await.unwrap();
    let root = root.to_str().unwrap().to_owned();
    let key = tokio::task::spawn_blocking(PrivateKey::try_generate)
        .await
        .unwrap()
        .unwrap();
    let key = Arc::new(key);
    for encrypted in [false, true] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(serve(
            listener,
            encrypted.then(|| config.clone()),
            key.clone(),
        ));
        let mut options = OptsBuilder::default()
            .ip_or_hostname("127.0.0.1")
            .tcp_port(port)
            .prefer_socket(false)
            .user(Some("pdo_test"))
            .pass(Some(PASSWORD))
            .max_allowed_packet(Some(1024 * 1024))
            .wait_timeout(Some(1));
        if encrypted {
            let settings = MysqlTls {
                ca: Some(root.clone()),
                capath: None,
                cert: None,
                key: None,
                verify: true,
            };
            options = options.ssl_opts(tls::options(&settings).await.unwrap());
        }
        let connection =
            tokio::time::timeout(std::time::Duration::from_secs(5), Conn::new(options))
                .await
                .unwrap()
                .unwrap();
        connection.disconnect().await.unwrap();
        server.await.unwrap();
    }
}

async fn serve(listener: TcpListener, tls: Option<Arc<ServerConfig>>, key: Arc<PrivateKey>) {
    let (mut socket, _) = listener.accept().await.unwrap();
    send(&mut socket, 0, &greeting(tls.is_some())).await;
    if let Some(config) = tls {
        let (sequence, request) = receive(&mut socket).await;
        assert_eq!(sequence, 1);
        assert_eq!(request.len(), 32);
        let encrypted = TlsAcceptor::from(config).accept(socket).await.unwrap();
        authenticate(encrypted, None).await;
    } else {
        authenticate(socket, Some(key.as_ref())).await;
    }
}

fn greeting(tls: bool) -> Vec<u8> {
    let mut payload = b"\x0a8.4.0\0\x01\0\0\0".to_vec();
    payload.extend_from_slice(&NONCE[..8]);
    payload.push(0);
    let capabilities: u32 = 0x0008_a201 | if tls { 0x0800 } else { 0 };
    payload.extend_from_slice(&(capabilities as u16).to_le_bytes());
    payload.extend_from_slice(&[45, 2, 0]);
    payload.extend_from_slice(&((capabilities >> 16) as u16).to_le_bytes());
    payload.push(21);
    payload.extend_from_slice(&[0; 10]);
    payload.extend_from_slice(&NONCE[8..]);
    payload.extend_from_slice(b"\0caching_sha2_password\0");
    payload
}

async fn authenticate(mut socket: impl AsyncRead + AsyncWrite + Unpin, rsa: Option<&PrivateKey>) {
    let (handshake, _) = receive(&mut socket).await;
    send(&mut socket, handshake + 1, &[1, 4]).await;
    let (mut sequence, mut password) = receive(&mut socket).await;
    if let Some(key) = rsa {
        assert_eq!(password, [2], "client requests the RSA key");
        let pem = key
            .to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .unwrap();
        let mut packet = vec![1];
        packet.extend_from_slice(pem.as_bytes());
        send(&mut socket, sequence + 1, &packet).await;
        let (encrypted_sequence, encrypted) = receive(&mut socket).await;
        sequence = encrypted_sequence;
        password = key.decrypt(Oaep::<Sha1>::new(), &encrypted).unwrap();
        for (index, byte) in password.iter_mut().enumerate() {
            *byte ^= NONCE[index % NONCE.len()];
        }
    }
    assert_eq!(password, [PASSWORD.as_bytes(), &[0]].concat());
    send(&mut socket, sequence + 1, &[0, 0, 0, 2, 0, 0, 0]).await;
    let (_, quit) = receive(&mut socket).await;
    assert_eq!(quit, [1]);
}

async fn send(socket: &mut (impl AsyncWrite + Unpin), sequence: u8, body: &[u8]) {
    let [a, b, c, _] = (body.len() as u32).to_le_bytes();
    socket.write_all(&[a, b, c, sequence]).await.unwrap();
    socket.write_all(body).await.unwrap();
    socket.flush().await.unwrap();
}

async fn receive(socket: &mut (impl AsyncRead + Unpin)) -> (u8, Vec<u8>) {
    let mut header = [0; 4];
    socket.read_exact(&mut header).await.unwrap();
    let size = u32::from_le_bytes([header[0], header[1], header[2], 0]);
    let mut body = vec![0; size as usize];
    socket.read_exact(&mut body).await.unwrap();
    (header[3], body)
}
