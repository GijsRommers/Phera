//! `curl_version()['protocols']` lists the schemes Phera can transfer.

use phera_extension_api::{ArrayKey, Context, NativeResult, PhpArray, Value};

use crate::constants::ids::*;

pub(crate) fn version(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    let mut protocols = PhpArray::new();
    protocols.push(Value::from("http"))?;
    protocols.push(Value::from("https"))?;
    let feature_list = [
        ("AsynchDNS", CURL_VERSION_ASYNCHDNS),
        ("CharConv", 0),
        ("Debug", 0),
        ("GSS-Negotiate", 0),
        ("IDN", CURL_VERSION_IDN),
        ("IPv6", CURL_VERSION_IPV6),
        ("krb4", 0),
        ("Largefile", CURL_VERSION_LARGEFILE),
        ("libz", CURL_VERSION_LIBZ),
        ("NTLM", 0),
        ("NTLMWB", 0),
        ("SPNEGO", 0),
        ("SSL", CURL_VERSION_SSL),
        ("SSPI", 0),
        ("TLS-SRP", 0),
        ("HTTP2", CURL_VERSION_HTTP2),
        ("GSSAPI", 0),
        ("KERBEROS5", 0),
        ("UNIX_SOCKETS", CURL_VERSION_UNIX_SOCKETS),
        ("PSL", CURL_VERSION_PSL),
        ("HTTPS_PROXY", 0),
        ("MULTI_SSL", 0),
        ("BROTLI", CURL_VERSION_BROTLI),
        ("ALTSVC", 0),
        ("HTTP3", 0),
        ("UNICODE", 0),
        ("ZSTD", 0),
        ("HSTS", 0),
        ("GSASL", 0),
    ];
    let mut names = PhpArray::new();
    let mut features = 0;
    for (name, bit) in feature_list {
        features |= bit;
        names.insert(ArrayKey::str(name), Value::Bool(bit != 0));
    }
    // SDKs compare the numeric prefix as a cURL API compatibility floor.
    // The backend and actual Phera version remain visible in the same string.
    let mut version = PhpArray::new();
    for (name, value) in [
        ("version_number", Value::Int(0x07_22_00)),
        ("age", Value::Int(11)),
        ("features", Value::Int(features)),
        ("feature_list", Value::from(names)),
        ("ssl_version_number", Value::Int(0)),
        (
            "version",
            Value::from(concat!("7.34.0-pl-Phera/", env!("CARGO_PKG_VERSION"))),
        ),
        (
            "host",
            Value::from(format!(
                "{}-{}",
                std::env::consts::ARCH,
                std::env::consts::OS
            )),
        ),
        ("ssl_version", Value::from("rustls/ring")),
        ("libz_version", Value::from("miniz_oxide")),
        ("protocols", Value::from(protocols)),
        ("ares", Value::from("")),
        ("ares_num", Value::Int(0)),
        ("libidn", Value::from("")),
        ("iconv_ver_num", Value::Int(0)),
        ("libssh_version", Value::from("")),
        ("brotli_ver_num", Value::Int(0)),
        ("brotli_version", Value::from("brotli (Rust)")),
    ] {
        version.insert(ArrayKey::str(name), value);
    }
    Ok(Value::from(version))
}
