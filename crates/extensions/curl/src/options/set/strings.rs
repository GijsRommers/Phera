//! `CURLOPT_POSTFIELDS` keeps binary data. String-valued options reject NUL bytes.

use crate::constants::ids::*;
use crate::options::{Protocols, Settings};

use super::Change;

pub(super) fn takes_text(option: i64) -> bool {
    matches!(
        option,
        CURLOPT_URL
            | CURLOPT_CUSTOMREQUEST
            | CURLOPT_POSTFIELDS
            | CURLOPT_ACCEPT_ENCODING
            | CURLOPT_USERAGENT
            | CURLOPT_REFERER
            | CURLOPT_COOKIE
            | CURLOPT_COOKIEFILE
            | CURLOPT_COOKIEJAR
            | CURLOPT_COOKIELIST
            | CURLOPT_USERPWD
            | CURLOPT_USERNAME
            | CURLOPT_PASSWORD
            | CURLOPT_XOAUTH2_BEARER
            | CURLOPT_PROXY
            | CURLOPT_NOPROXY
            | CURLOPT_PROXYUSERPWD
            | CURLOPT_UNIX_SOCKET_PATH
            | CURLOPT_CAINFO
            | CURLOPT_PROTOCOLS_STR
            | CURLOPT_REDIR_PROTOCOLS_STR
            | CURLOPT_DEFAULT_PROTOCOL
    )
}

pub(super) fn text(bytes: Option<Vec<u8>>) -> Option<String> {
    bytes.map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

pub(super) fn text_list(list: Vec<Vec<u8>>) -> Vec<String> {
    list.into_iter()
        .map(|value| String::from_utf8_lossy(&value).into_owned())
        .collect()
}

pub(super) fn change(settings: &mut Settings, option: i64, bytes: Option<Vec<u8>>) -> Change {
    match option {
        CURLOPT_URL => settings.url = text(bytes),
        CURLOPT_CUSTOMREQUEST => settings.custom_method = text(bytes),
        CURLOPT_ACCEPT_ENCODING => settings.encoding = text(bytes),
        CURLOPT_USERAGENT => settings.agent = bytes,
        CURLOPT_REFERER => settings.referer = bytes,
        CURLOPT_COOKIE => settings.cookie = bytes,
        CURLOPT_COOKIEFILE => {
            if let Some(path) = bytes {
                settings.cookie_files.push(path);
            } else {
                settings.cookie_files.clear();
            }
        }
        CURLOPT_COOKIEJAR => settings.cookie_jar = bytes,
        CURLOPT_COOKIELIST => return Change::Cookie(bytes.unwrap_or_default()),
        CURLOPT_USERPWD => {
            if let Some(bytes) = bytes {
                let mut fields = bytes.splitn(2, |byte| *byte == b':');
                settings.username = fields.next().map(<[u8]>::to_vec);
                settings.password = fields.next().unwrap_or_default().to_vec();
            } else {
                settings.username = None;
                settings.password.clear();
            }
        }
        CURLOPT_USERNAME => settings.username = bytes,
        CURLOPT_PASSWORD => settings.password = bytes.unwrap_or_default(),
        CURLOPT_XOAUTH2_BEARER => settings.bearer = bytes,
        CURLOPT_DEFAULT_PROTOCOL => {
            let protocol = text(bytes).unwrap_or_else(|| "http".into());
            match protocol.to_ascii_lowercase().as_str() {
                "http" | "https" => settings.default_protocol = protocol.to_ascii_lowercase(),
                _ => return Change::Refused("only HTTP and HTTPS are available"),
            }
        }
        CURLOPT_PROTOCOLS_STR | CURLOPT_REDIR_PROTOCOLS_STR => {
            let names = text(bytes)
                .unwrap_or_else(|| "all".into())
                .to_ascii_lowercase();
            let protocols = if names == "all" {
                Protocols::default()
            } else {
                let mut protocols = Protocols {
                    http: false,
                    https: false,
                };
                for name in names.split(',') {
                    match name {
                        "http" => protocols.http = true,
                        "https" => protocols.https = true,
                        _ => return Change::Refused("only HTTP and HTTPS are available"),
                    }
                }
                protocols
            };
            if option == CURLOPT_PROTOCOLS_STR {
                settings.protocols = protocols;
            } else {
                settings.redirect_protocols = protocols;
            }
        }
        _ => return Change::Refused("option is unavailable in Phera's HTTP client"),
    }
    Change::Applied
}
