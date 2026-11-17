//! URL validation supports PHP's legacy parser and its two URI parser options.
//!
//! ```php
//! filter_var('https://example.org?', FILTER_VALIDATE_URL, FILTER_FLAG_QUERY_REQUIRED);
//! // 'https://example.org?' — the query exists, even though it is empty
//! ```

use phera_extension_api::NativeError;

use crate::constants::{PATH_REQUIRED, QUERY_REQUIRED};
use crate::options::Options;
use crate::{domain, ip, sanitize};

pub fn valid(input: &[u8], options: &Options) -> Result<bool, NativeError> {
    if !input.iter().copied().all(sanitize::url_byte) {
        return Ok(false);
    }
    let Ok(text) = std::str::from_utf8(input) else {
        return Ok(false);
    };
    let parser = options.string("uri_parser_class");
    let components = match parser.as_ref().map(|name| name.as_bytes()) {
        None | Some(b"parse_url") => legacy(text),
        Some(b"Uri\\Rfc3986\\Uri") => fluent_uri::Uri::parse(text).ok().map(|uri| Components {
            host: uri.authority().is_some(),
            path: true,
            query: uri.query().is_some(),
            host_optional: optional_host(uri.scheme().as_str()),
        }),
        Some(b"Uri\\WhatWg\\Url") => ::url::Url::parse(text).ok().map(|url| {
            let explicit_authority = text
                .split_once(':')
                .is_some_and(|(_, rest)| rest.starts_with("//"));
            Components {
                host: url.has_host() || explicit_authority,
                path: true,
                query: url.query().is_some(),
                host_optional: optional_host(url.scheme()),
            }
        }),
        Some(_) => return Err(options.invalid("\"uri_parser_class\" option has invalid value")),
    };

    Ok(components.is_some_and(|parts| {
        (parts.host || parts.host_optional)
            && (parts.path || !options.flags.has(PATH_REQUIRED))
            && (parts.query || !options.flags.has(QUERY_REQUIRED))
    }))
}

struct Components {
    host: bool,
    path: bool,
    query: bool,
    host_optional: bool,
}

fn optional_host(scheme: &str) -> bool {
    matches!(scheme, "mailto" | "news" | "file")
}

fn legacy(text: &str) -> Option<Components> {
    let (scheme, rest) = text.split_once(':')?;
    if scheme.is_empty()
        || !scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+.-".contains(&byte))
    {
        return None;
    }
    // The legacy parser treats scheme:123 and scheme:123/path as host:port.
    if rest.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits <= 5 && (digits == rest.len() || rest.as_bytes()[digits] == b'/') {
            return None;
        }
    }
    let (host, tail) = match rest.strip_prefix("//") {
        Some(tail) if scheme.eq_ignore_ascii_case("file") && tail.starts_with('/') => (None, tail),
        Some(tail) => {
            let end = tail.find(['/', '?', '#']).unwrap_or(tail.len());
            (Some(authority(&tail[..end])?), &tail[end..])
        }
        None => (None, rest),
    };

    if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
        let host = host?;
        if !domain::valid(host.as_bytes(), true)
            && !host
                .strip_prefix('[')
                .and_then(|host| host.strip_suffix(']'))
                .is_some_and(|literal| ip::ipv6_literal(literal.as_bytes()))
        {
            return None;
        }
    }
    let before_fragment = tail.split_once('#').map_or(tail, |(head, _)| head);
    let (path, query) = before_fragment
        .split_once('?')
        .map_or((before_fragment, false), |(head, _)| (head, true));
    Some(Components {
        host: host.is_some(),
        path: !path.is_empty(),
        query,
        host_optional: optional_host(scheme),
    })
}

fn authority(text: &str) -> Option<&str> {
    let host = match text.rsplit_once('@') {
        Some((userinfo, host)) => {
            let (user, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
            if !valid_userinfo(user.as_bytes()) || !valid_userinfo(password.as_bytes()) {
                return None;
            }
            host
        }
        None => text,
    };
    let host = if host.starts_with('[') && host.ends_with(']') {
        host
    } else if let Some((host, port)) = host.rsplit_once(':') {
        if !valid_port(port) {
            return None;
        }
        host
    } else {
        host
    };
    (!host.is_empty()).then_some(host)
}

fn valid_port(port: &str) -> bool {
    if port.is_empty() {
        return true;
    }
    if port.len() > 5 {
        return false;
    }
    let digits = port.trim_start_matches(['+', '-']);
    let end = digits.bytes().take_while(u8::is_ascii_digit).count();
    let port = &port[..port.len() - digits.len() + end];
    port.parse::<u16>().is_ok() || port == "-0"
}

fn valid_userinfo(mut input: &[u8]) -> bool {
    while let [byte, tail @ ..] = input {
        input = if byte.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:".contains(byte) {
            tail
        } else if let (b'%', [first, second, tail @ ..]) = (byte, tail)
            && first.is_ascii_digit()
            && second.is_ascii_hexdigit()
        {
            tail
        } else {
            return false;
        };
    }
    true
}
