//! `stream_get_meta_data($socket)['crypto']` reports the negotiated TLS session.

use phera_extension_api::{ArrayKey, PhpArray, Value};
use rustls::{CipherSuite, ProtocolVersion};

use super::{Connection, State};

pub(in crate::socket) fn crypto(socket: &Connection) -> Option<Value> {
    let secure = socket.secure.try_lock().ok()?;
    let State::Established(stream) = &*secure else {
        return None;
    };
    let (_, session) = stream.get_ref();
    let protocol = match session.protocol_version()? {
        ProtocolVersion::TLSv1_2 => "TLSv1.2",
        ProtocolVersion::TLSv1_3 => "TLSv1.3",
        _ => return None,
    };
    let (cipher, bits) = suite(session.negotiated_cipher_suite()?.suite())?;
    let mut metadata = PhpArray::new();
    for (name, value) in [
        ("protocol", Value::from(protocol)),
        ("cipher_name", Value::from(cipher)),
        ("cipher_bits", Value::Int(bits)),
        ("cipher_version", Value::from(protocol)),
    ] {
        metadata.insert(ArrayKey::str(name), value);
    }
    Some(Value::from(metadata))
}

fn suite(suite: CipherSuite) -> Option<(&'static str, i64)> {
    use CipherSuite::*;

    Some(match suite {
        TLS13_AES_128_GCM_SHA256 => ("TLS_AES_128_GCM_SHA256", 128),
        TLS13_AES_256_GCM_SHA384 => ("TLS_AES_256_GCM_SHA384", 256),
        TLS13_CHACHA20_POLY1305_SHA256 => ("TLS_CHACHA20_POLY1305_SHA256", 256),
        TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256 => ("ECDHE-ECDSA-AES128-GCM-SHA256", 128),
        TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384 => ("ECDHE-ECDSA-AES256-GCM-SHA384", 256),
        TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 => ("ECDHE-RSA-AES128-GCM-SHA256", 128),
        TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384 => ("ECDHE-RSA-AES256-GCM-SHA384", 256),
        TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256 => ("ECDHE-ECDSA-CHACHA20-POLY1305", 256),
        TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256 => ("ECDHE-RSA-CHACHA20-POLY1305", 256),
        _ => return None,
    })
}
