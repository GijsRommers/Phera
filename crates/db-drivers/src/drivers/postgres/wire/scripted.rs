//! A local scripted peer for the worker's PostgreSQL socket.

use bytes::{BufMut, BytesMut};
use postgres_protocol::authentication::md5_hash;
use postgres_protocol::message::backend::Message;
use ring::{hmac, pbkdf2};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

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

async fn frontend(peer: &mut DuplexStream, tag: Option<u8>) -> Vec<u8> {
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

#[tokio::test]
async fn trust_cleartext_and_md5_complete_startup() {
    for mode in [0, 3, 5] {
        let (socket, mut peer) = tokio::io::duplex(4096);
        let mut transport = Transport::new(Box::new(socket));
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
        let client = startup::start(&mut transport, &[("user", USER)], USER, PASSWORD);
        let (_, session) = tokio::join!(server, client);
        let session = session.unwrap();
        assert_eq!(session.parameters["server_version"], "test-server");
        assert_eq!((session.pid, session.status), (21, b'I'));
    }
}

#[tokio::test]
async fn scram_checks_the_final_proof_and_does_not_offer_unavailable_channel_binding() {
    assert!(script_scram(true).await.is_ok());
    assert!(matches!(
        script_scram(false).await,
        Err(startup::StartError::Transport(_))
    ));
}

async fn script_scram(valid_proof: bool) -> Result<startup::Session, startup::StartError> {
    let (socket, mut peer) = tokio::io::duplex(4096);
    let mut transport = Transport::new(Box::new(socket));
    let server = async {
        frontend(&mut peer, None).await;
        let mechanisms = b"SCRAM-SHA-256-PLUS\0SCRAM-SHA-256\0\0";
        peer.write_all(&auth(10, mechanisms)).await.unwrap();
        let initial = frontend(&mut peer, Some(b'p')).await;
        let rest = initial.strip_prefix(b"SCRAM-SHA-256\0").unwrap();
        let request = std::str::from_utf8(&rest[4..]).unwrap();
        let bare = request.strip_prefix("n,,").unwrap();
        let (_, nonce) = request.rsplit_once(",r=").unwrap();
        let first = format!("r={nonce}server,s=c2FsdA==,i=4096");
        peer.write_all(&auth(11, first.as_bytes())).await.unwrap();
        let final_message = frontend(&mut peer, Some(b'p')).await;
        let (without_proof, _) = std::str::from_utf8(&final_message)
            .unwrap()
            .rsplit_once(",p=")
            .unwrap();
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
        use base64::Engine;
        let proof = if valid_proof {
            proof.as_ref()
        } else {
            &[0; 32]
        };
        let final_message = format!(
            "v={}",
            base64::engine::general_purpose::STANDARD.encode(proof)
        );
        let mut reply = auth(12, final_message.as_bytes());
        reply.unsplit(startup_done());
        peer.write_all(&reply).await.unwrap();
    };
    let client = startup::start(&mut transport, &[("user", USER)], USER, PASSWORD);
    let (_, result) = tokio::join!(server, client);
    result
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
        let client = startup::start(&mut transport, &[("user", USER)], USER, PASSWORD);
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
        let session = startup::start(&mut transport, &[("user", USER)], USER, PASSWORD)
            .await
            .unwrap();
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
