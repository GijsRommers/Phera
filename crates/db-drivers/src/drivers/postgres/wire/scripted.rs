//! A local scripted peer for the worker's PostgreSQL socket.

use bytes::{BufMut, BytesMut};
use postgres_protocol::authentication::md5_hash;
use postgres_protocol::message::backend::Message;
use ring::{hmac, pbkdf2};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, DuplexStream};

use super::{Client, Transport, startup};
use crate::parameters::BoundValue;

const USER: &str = "scripted-user";
const PASSWORD: &[u8] = b"scripted password";

pub(super) fn frame(tag: u8, body: &[u8], out: &mut BytesMut) {
    out.put_u8(tag);
    out.put_u32(u32::try_from(body.len() + 4).unwrap());
    out.extend_from_slice(body);
}

fn auth(code: u32, data: &[u8]) -> BytesMut {
    let mut body = BytesMut::new();
    body.put_u32(code);
    body.extend_from_slice(data);
    let mut reply = BytesMut::new();
    frame(b'R', &body, &mut reply);
    reply
}

async fn frontend(peer: &mut (impl AsyncRead + Unpin + ?Sized), tag: Option<u8>) -> Vec<u8> {
    if let Some(tag) = tag {
        assert_eq!(peer.read_u8().await.unwrap(), tag);
    }
    let len = usize::try_from(peer.read_u32().await.unwrap()).unwrap();
    assert!((4..4096).contains(&len));
    let mut body = vec![0; len - 4];
    peer.read_exact(&mut body).await.unwrap();
    body
}

fn startup_done() -> BytesMut {
    let mut reply = auth(0, b"");
    frame(b'S', b"server_version\0test-server\0", &mut reply);
    frame(b'K', b"\0\0\0\x15\0\0\0\x09", &mut reply);
    frame(b'Z', b"I", &mut reply);
    reply
}

async fn start_default(transport: &mut Transport) -> Result<startup::Session, startup::StartError> {
    startup::start(
        transport,
        &[("user", USER)],
        USER,
        PASSWORD,
        &startup::AuthPolicy::default(),
        None,
    )
    .await
}

#[tokio::test]
async fn trust_cleartext_and_md5_complete_startup() {
    for (mode, requirement) in [(0, ""), (3, "none,password"), (5, "!password,!none")] {
        let (socket, mut peer) = tokio::io::duplex(4096);
        let mut transport = Transport::new(Box::new(socket));
        let dsn = dsn(&format!("require_auth='{requirement}'"));
        let policy = startup::AuthPolicy::parse(&dsn).unwrap();
        let server = async {
            let startup = frontend(&mut peer, None).await;
            assert_eq!(&startup[..4], b"\0\x03\0\0");
            assert!(startup.ends_with(b"user\0scripted-user\0\0"));
            let salt = *b"salt";
            if mode != 0 {
                let data = if mode == 5 { salt.as_slice() } else { b"" };
                peer.write_all(&auth(mode, data)).await.unwrap();
                let mut expected = if mode == 5 {
                    md5_hash(USER.as_bytes(), PASSWORD, salt).into_bytes()
                } else {
                    PASSWORD.to_vec()
                };
                expected.push(0);
                assert_eq!(frontend(&mut peer, Some(b'p')).await, expected);
            }
            peer.write_all(&startup_done()).await.unwrap();
        };
        let client = startup::start(
            &mut transport,
            &[("user", USER)],
            USER,
            PASSWORD,
            &policy,
            None,
        );
        let (_, session) = tokio::join!(server, client);
        let session = session.unwrap();
        assert_eq!(
            session.parameters.get("server_version"),
            Some("test-server")
        );
        assert_eq!((session.pid, session.status), (21, b'I'));
    }
}

#[derive(Clone, Copy)]
enum ScramEnd {
    Valid,
    InvalidProof,
    ContinueTwice,
}

#[tokio::test]
async fn scram_requires_an_in_order_server_proof() {
    assert!(script_scram(ScramEnd::Valid).await.is_ok());
    for end in [ScramEnd::InvalidProof, ScramEnd::ContinueTwice] {
        assert!(matches!(
            script_scram(end).await,
            Err(startup::StartError::Transport(_))
        ));
    }
}

async fn script_scram(end: ScramEnd) -> Result<startup::Session, startup::StartError> {
    let (socket, mut peer) = tokio::io::duplex(4096);
    let mut transport = Transport::new(Box::new(socket));
    let server = scram_peer(
        &mut peer,
        b"SCRAM-SHA-256-PLUS\0SCRAM-SHA-256\0\0",
        Some(("SCRAM-SHA-256", "n,,", &[])),
        end,
    );
    let (_, result) = tokio::join!(server, start_default(&mut transport));
    result
}

async fn scram_peer(
    peer: &mut dyn super::super::stream::Socket,
    mechanisms: &[u8],
    expected: Option<(&str, &str, &[u8])>,
    end: ScramEnd,
) {
    frontend(peer, None).await;
    peer.write_all(&auth(10, mechanisms)).await.unwrap();
    peer.flush().await.unwrap();
    let Some((mechanism, header, certificate_hash)) = expected else {
        assert!(
            peer.read_u8().await.is_err(),
            "authentication must end without a reply"
        );
        return;
    };
    let initial = frontend(peer, Some(b'p')).await;
    let rest = initial
        .strip_prefix(format!("{mechanism}\0").as_bytes())
        .unwrap();
    let request = std::str::from_utf8(&rest[4..]).unwrap();
    let bare = request.strip_prefix(header).unwrap();
    let (_, nonce) = request.rsplit_once(",r=").unwrap();
    let first = format!("r={nonce}server,s=c2FsdA==,i=4096");
    peer.write_all(&auth(11, first.as_bytes())).await.unwrap();
    peer.flush().await.unwrap();
    let final_message = frontend(peer, Some(b'p')).await;
    let (without_proof, _) = std::str::from_utf8(&final_message)
        .unwrap()
        .rsplit_once(",p=")
        .unwrap();
    use base64::Engine;
    let base64 = base64::engine::general_purpose::STANDARD;
    let encoded_binding = without_proof
        .strip_prefix("c=")
        .unwrap()
        .split(',')
        .next()
        .unwrap();
    assert_eq!(
        base64.decode(encoded_binding).unwrap(),
        [header.as_bytes(), certificate_hash].concat(),
    );
    if matches!(end, ScramEnd::ContinueTwice) {
        peer.write_all(&auth(11, first.as_bytes())).await.unwrap();
        peer.flush().await.unwrap();
        return;
    }
    let mut salted = [0; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        4096.try_into().unwrap(),
        b"salt",
        PASSWORD,
        &mut salted,
    );
    let server_key = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, &salted), b"Server Key");
    let transcript = format!("{bare},{first},{without_proof}");
    let proof = hmac::sign(
        &hmac::Key::new(hmac::HMAC_SHA256, server_key.as_ref()),
        transcript.as_bytes(),
    );
    let proof = if matches!(end, ScramEnd::Valid) {
        proof.as_ref()
    } else {
        &[0; 32]
    };
    let final_message = format!("v={}", base64.encode(proof));
    let mut reply = auth(12, final_message.as_bytes());
    reply.unsplit(startup_done());
    peer.write_all(&reply).await.unwrap();
    peer.flush().await.unwrap();
}

#[tokio::test]
async fn channel_binding_modes_use_the_negotiated_transport() {
    use crate::drivers::tls::certificates::certificates;
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;

    let certificates = certificates("localhost");
    let config = certificates.server(false);
    let hash = ring::digest::digest(&ring::digest::SHA256, certificates.server_der());
    let plain_only = b"SCRAM-SHA-256\0\0".as_slice();
    let both = b"SCRAM-SHA-256-PLUS\0SCRAM-SHA-256\0\0".as_slice();
    let plain = Some(("SCRAM-SHA-256", "n,,", &[][..]));
    let bound = Some((
        "SCRAM-SHA-256-PLUS",
        "p=tls-server-end-point,,",
        hash.as_ref(),
    ));
    let unrequested = Some(("SCRAM-SHA-256", "y,,", &[][..]));
    let password = std::str::from_utf8(PASSWORD).unwrap();
    for (mode, tls, mechanisms, expected) in [
        ("disable", false, both, plain),
        ("prefer", false, both, plain),
        ("require", false, both, None),
        ("disable", true, both, plain),
        ("prefer", true, both, bound),
        ("prefer", true, plain_only, unrequested),
        ("require", true, both, bound),
        ("require", true, plain_only, None),
    ] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let ssl = if tls { "require" } else { "disable" };
        let dsn = dsn(&format!(
            "host=127.0.0.1 port={port} sslmode={ssl} channel_binding={mode} require_auth=md5,scram-sha-256"
        ));
        let server = async {
            let (mut socket, _) = listener.accept().await.unwrap();
            if tls {
                let mut request = [0; 8];
                socket.read_exact(&mut request).await.unwrap();
                assert_eq!(request, [0, 0, 0, 8, 4, 210, 22, 47]);
                socket.write_all(b"S").await.unwrap();
                let mut peer = TlsAcceptor::from(config.clone())
                    .accept(socket)
                    .await
                    .unwrap();
                scram_peer(&mut peer, mechanisms, expected, ScramEnd::Valid).await;
            } else {
                scram_peer(&mut socket, mechanisms, expected, ScramEnd::Valid).await;
            }
        };
        let client = super::super::Postgres::connect(&dsn, USER, password);
        let (_, result) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(server, client)
        })
        .await
        .unwrap();
        assert_eq!(result.is_ok(), expected.is_some(), "{mode}, TLS={tls}");
    }
}

fn dsn(fields: &str) -> crate::dsn::Dsn {
    crate::dsn::Dsn {
        driver: crate::dsn::DriverName::Postgres,
        fields: crate::dsn::postgres::fields(fields, false).unwrap(),
        options: Default::default(),
    }
}

#[tokio::test]
async fn disallowed_authentication_sends_no_password_or_proof() {
    for (require, challenge) in [
        ("scram-sha-256", auth(3, b"")),
        ("!password,!md5", auth(5, b"salt")),
        ("none", auth(10, b"SCRAM-SHA-256\0\0")),
        ("!none", auth(0, b"")),
    ] {
        let (socket, mut peer) = tokio::io::duplex(4096);
        let dsn = dsn(&format!("require_auth={require}"));
        let policy = startup::AuthPolicy::parse(&dsn).unwrap();
        let server = async {
            frontend(&mut peer, None).await;
            peer.write_all(&challenge).await.unwrap();
            assert!(
                peer.read_u8().await.is_err(),
                "a disallowed method received a reply"
            );
        };
        let client = async move {
            let mut transport = Transport::new(Box::new(socket));
            startup::start(
                &mut transport,
                &[("user", USER)],
                USER,
                PASSWORD,
                &policy,
                None,
            )
            .await
        };
        let (_, result) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(server, client)
        })
        .await
        .unwrap();
        assert!(
            matches!(result, Err(startup::StartError::Transport(_))),
            "{require}"
        );
    }
}

#[tokio::test]
async fn unsupported_authentication_ends_startup() {
    for request in [auth(7, b""), auth(10, b"SCRAM-SHA-256-PLUS\0\0")] {
        let (socket, mut peer) = tokio::io::duplex(4096);
        let mut transport = Transport::new(Box::new(socket));
        let server = async {
            frontend(&mut peer, None).await;
            peer.write_all(&request).await.unwrap();
        };
        let client = start_default(&mut transport);
        let (_, result) = tokio::join!(server, client);
        assert!(matches!(result, Err(startup::StartError::Transport(_))));
    }
}

#[tokio::test]
async fn first_execute_requests_text_and_drains_an_error_mid_result() {
    let (socket, mut peer) = tokio::io::duplex(4096);
    let server = async {
        frontend(&mut peer, None).await;
        peer.write_all(&startup_done()).await.unwrap();
        let parse = frontend(&mut peer, Some(b'P')).await;
        assert!(parse.starts_with(b"prepared\0SELECT $1::int\0"));
        let bind = frontend(&mut peer, Some(b'B')).await;
        assert_eq!(bind, b"\0prepared\0\0\x01\0\0\0\x01\0\0\0\x013\0\x01\0\0");
        assert_eq!(frontend(&mut peer, Some(b'D')).await, b"P\0");
        assert_eq!(frontend(&mut peer, Some(b'E')).await, b"\0\0\0\0\0");
        assert!(frontend(&mut peer, Some(b'S')).await.is_empty());
        let mut response = BytesMut::new();
        frame(b'1', b"", &mut response);
        frame(b'2', b"", &mut response);
        frame(b'D', b"\0\x01\0\0\0\x013", &mut response);
        frame(
            b'E',
            b"SERROR\0C22012\0Mdivision by zero\0\0",
            &mut response,
        );
        frame(b'Z', b"E", &mut response);
        peer.write_all(&response).await.unwrap();
        assert_eq!(frontend(&mut peer, Some(b'Q')).await, b"ROLLBACK\0");
        response.clear();
        frame(b'C', b"ROLLBACK\0", &mut response);
        frame(b'Z', b"I", &mut response);
        peer.write_all(&response).await.unwrap();
    };
    let client = async {
        let mut transport = Transport::new(Box::new(socket));
        let session = start_default(&mut transport).await.unwrap();
        let mut client = Client::new(transport, session);
        client
            .extended(
                "prepared",
                Some("SELECT $1::int"),
                &[BoundValue::Int(3)],
                &[],
            )
            .await
            .unwrap();
        assert!(matches!(client.next().await, Ok(Message::ParseComplete)));
        assert!(matches!(client.next().await, Ok(Message::BindComplete)));
        assert!(matches!(client.next().await, Ok(Message::DataRow(_))));
        let Ok(Message::ErrorResponse(error)) = client.next().await else {
            panic!("expected server error");
        };
        assert_eq!(
            client.failure(error, "SELECT $1::int", false).await.state,
            "22012"
        );
        assert_eq!(client.status(), b'E');
        client.simple("ROLLBACK").await.unwrap();
        assert!(matches!(
            client.next().await,
            Ok(Message::CommandComplete(_))
        ));
        assert!(matches!(client.next().await, Ok(Message::ReadyForQuery(_))));
        assert_eq!(client.status(), b'I');
    };
    tokio::join!(server, client);
}

async fn client_and_peer() -> (Client, DuplexStream) {
    let (socket, mut peer) = tokio::io::duplex(16 * 1024);
    let mut transport = Transport::new(Box::new(socket));
    let server = async {
        frontend(&mut peer, None).await;
        peer.write_all(&startup_done()).await.unwrap();
    };
    let (_, session) = tokio::join!(server, start_default(&mut transport));
    (Client::new(transport, session.unwrap()), peer)
}

#[tokio::test]
async fn malformed_frame_tags_and_counts_retire_the_socket() {
    for bytes in [b"X\0\0\0\x04".as_slice(), b"T\0\0\0\x06\xff\xff"] {
        let (mut client, mut peer) = client_and_peer().await;
        peer.write_all(bytes).await.unwrap();
        assert!(client.next().await.is_err());
        assert!(client.is_closed());
    }
}

#[tokio::test]
async fn data_rows_validate_cell_lengths_and_width() {
    use crate::drivers::{Column, RowBuffer};
    use crate::metadata::{Metadata, PostgresColumn};
    let column = Column {
        name: "a".into(),
        metadata: Metadata::Postgres(PostgresColumn {
            oid: 23,
            table_oid: 0,
            modifier: -1,
            native_type: "int4".into(),
        }),
    };
    for malformed in [
        b"\0\x01\x7f\xff\xff\xff".as_slice(),
        b"\0\x01\xff\xff\xff\xfe",
        b"\0\x02\0\0\0\x011\0\0\0\x012",
    ] {
        let (mut client, mut peer) = client_and_peer().await;
        let mut frame_bytes = BytesMut::new();
        frame(b'D', malformed, &mut frame_bytes);
        peer.write_all(&frame_bytes).await.unwrap();
        let Ok(Message::DataRow(row)) = client.next().await else {
            panic!("expected a parsed DataRow");
        };
        let mut values = RowBuffer::with_capacity(1, 0);
        let result = super::super::values::append(std::slice::from_ref(&column), &mut values, &row);
        assert_eq!(result.unwrap_err().state, "08006");
        assert!(values.is_empty());
    }
}

#[tokio::test]
async fn oversized_bind_leaves_the_socket_usable() {
    let (mut client, mut peer) = client_and_peer().await;
    let values = vec![BoundValue::Null; 70_000];
    assert!(
        client
            .extended("", Some("SELECT 1"), &values, &[])
            .await
            .is_err()
    );
    assert!(!client.is_closed());
    client.simple("SELECT 1").await.unwrap();
    assert_eq!(frontend(&mut peer, Some(b'Q')).await, b"SELECT 1\0");
}

#[tokio::test]
async fn asynchronous_queues_evict_oldest_by_count_and_bytes() {
    let (mut client, mut peer) = client_and_peer().await;
    let server = async {
        let mut small = BytesMut::new();
        for index in 0..4_100 {
            frame(
                b'N',
                format!("SNOTICE\0M{index}\0\0").as_bytes(),
                &mut small,
            );
        }
        frame(b'Z', b"I", &mut small);
        peer.write_all(&small).await.unwrap();
        let large = "x".repeat(3 * 1024 * 1024);
        let mut frames = BytesMut::new();
        for index in 0..2 {
            frame(
                b'N',
                format!("SNOTICE\0M{index}{large}\0\0").as_bytes(),
                &mut frames,
            );
            let mut notify = BytesMut::new();
            notify.put_i32(index);
            notify.extend_from_slice(b"channel\0");
            notify.extend_from_slice(large.as_bytes());
            notify.put_u8(0);
            frame(b'A', &notify, &mut frames);
        }
        frame(b'Z', b"I", &mut frames);
        peer.write_all(&frames).await.unwrap();
    };
    let client = async {
        assert!(matches!(client.next().await, Ok(Message::ReadyForQuery(_))));
        let notices = client.notices(false);
        assert_eq!(notices.len(), 4_096);
        assert!(notices[0].ends_with('4'));
        drop(notices);
        assert!(matches!(client.next().await, Ok(Message::ReadyForQuery(_))));
        let notices = client.notices(false);
        assert_eq!(notices.len(), 1);
        assert!(notices[0].starts_with("NOTICE:  1"));
        assert_eq!(client.notification().unwrap().process_id(), 1);
        assert!(client.notification().is_none());
    };
    tokio::join!(server, client);
}

#[tokio::test]
async fn oversized_parameter_status_retires_the_socket() {
    for flood in [false, true] {
        let (mut client, mut peer) = client_and_peer().await;
        let mut frames = BytesMut::new();
        if flood {
            for index in 0..129 {
                frame(b'S', format!("p{index}\0value\0").as_bytes(), &mut frames);
            }
        } else {
            let mut parameter = BytesMut::from(&b"client_encoding\0"[..]);
            parameter.resize(129 * 1024, b'x');
            parameter.put_u8(0);
            frame(b'S', &parameter, &mut frames);
        }
        let server = async {
            let _ = peer.write_all(&frames).await;
        };
        let client = async {
            assert!(client.next().await.is_err());
            assert!(client.is_closed());
        };
        tokio::join!(server, client);
    }
}
