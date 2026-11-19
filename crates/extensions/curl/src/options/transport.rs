//! `CURLOPT_HTTP_VERSION` and connection options determine pool compatibility.

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub(crate) enum HttpVersion {
    #[default]
    Auto,
    OneZero,
    OneOne,
    Two,
    TwoTls,
    TwoPrior,
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub(crate) enum IpVersion {
    #[default]
    Any,
    Four,
    Six,
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub(crate) enum TlsVersions {
    #[default]
    Modern,
    Tls12,
    Tls13,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct Transport {
    pub(crate) http: HttpVersion,
    pub(crate) ip: IpVersion,
    pub(crate) resolve: Vec<String>,
    pub(crate) connect_to: Vec<String>,
    pub(crate) proxy: Option<String>,
    pub(crate) proxy_port: Option<u16>,
    pub(crate) proxy_auth: bool,
    pub(crate) proxy_credentials: Option<Vec<u8>>,
    pub(crate) no_proxy: Option<String>,
    pub(crate) tunnel: bool,
    pub(crate) unix: Option<PathBuf>,
    pub(crate) ca_file: Option<PathBuf>,
    pub(crate) keepalive: bool,
    pub(crate) keep_idle: u64,
    pub(crate) keep_interval: u64,
    pub(crate) keep_count: u32,
    pub(crate) nodelay: bool,
    pub(crate) alpn: bool,
    pub(crate) tls: TlsVersions,
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            http: HttpVersion::Auto,
            ip: IpVersion::Any,
            resolve: Vec::new(),
            connect_to: Vec::new(),
            proxy: None,
            proxy_port: None,
            proxy_auth: true,
            proxy_credentials: None,
            no_proxy: None,
            tunnel: false,
            unix: None,
            ca_file: None,
            keepalive: false,
            keep_idle: 60,
            keep_interval: 60,
            keep_count: 9,
            nodelay: true,
            alpn: true,
            tls: TlsVersions::Modern,
        }
    }
}
