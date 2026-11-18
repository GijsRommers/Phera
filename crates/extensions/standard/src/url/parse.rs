//! PHP's URL splitter, including its host/port heuristics.
//!
//! `parse_url('example.com:80/')` has a host and port but no scheme.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, Value,
    args,
};

pub(super) const ENTRY: FunctionEntry = FunctionEntry::sync(
    "parse_url",
    &[
        Param::new("url", ParamType::String),
        Param::new("component", ParamType::Int).optional(),
    ],
    parse_url,
);

pub(super) const COMPONENTS: [(&str, &str); 8] = [
    ("PHP_URL_SCHEME", "scheme"),
    ("PHP_URL_HOST", "host"),
    ("PHP_URL_PORT", "port"),
    ("PHP_URL_USER", "user"),
    ("PHP_URL_PASS", "pass"),
    ("PHP_URL_PATH", "path"),
    ("PHP_URL_QUERY", "query"),
    ("PHP_URL_FRAGMENT", "fragment"),
];

#[derive(Default)]
struct Url<'a> {
    scheme: Option<&'a [u8]>,
    host: Option<&'a [u8]>,
    port: Option<i64>,
    user: Option<&'a [u8]>,
    pass: Option<&'a [u8]>,
    path: Option<&'a [u8]>,
    query: Option<&'a [u8]>,
    fragment: Option<&'a [u8]>,
}

fn parse_url(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let Some(url) = parse(args::bytes(arguments, 0)) else {
        return Ok(Value::Bool(false));
    };
    let component = args::int(arguments, 1).unwrap_or(-1);
    let components = [
        url.scheme.map(sanitize),
        url.host.map(sanitize),
        url.port.map(Value::Int),
        url.user.map(sanitize),
        url.pass.map(sanitize),
        url.path.map(sanitize),
        url.query.map(sanitize),
        url.fragment.map(sanitize),
    ];
    if component >= 0 {
        return components.into_iter().nth(component as usize).map_or_else(
            || {
                Err(NativeError::value_error(format!(
                    "parse_url(): Argument #2 ($component) must be a valid URL component \
                     identifier, {component} given"
                )))
            },
            |value| Ok(value.unwrap_or_default()),
        );
    }
    let mut result = PhpArray::new();
    for ((_, name), value) in COMPONENTS.into_iter().zip(components) {
        if let Some(value) = value {
            result.insert(ArrayKey::str(name), value);
        }
    }
    Ok(Value::from(result))
}

fn sanitize(bytes: &[u8]) -> Value {
    Value::from(
        bytes
            .iter()
            .map(|&byte| if byte.is_ascii_control() { b'_' } else { byte })
            .collect::<Vec<_>>(),
    )
}

fn parse(input: &[u8]) -> Option<Url<'_>> {
    let mut url = Url::default();
    let relative = input.starts_with(b"//");
    let Some(colon) = input.iter().position(|&byte| byte == b':') else {
        return if let Some(host) = input.strip_prefix(b"//") {
            url.authority(host)
        } else {
            Some(url.tail(input))
        };
    };
    let (before, after) = (&input[..colon], &input[colon + 1..]);
    if colon == 0 {
        return url.possible_port(input, after);
    }
    if !before
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || b"+.-".contains(byte))
    {
        return if !after.is_empty() && !before.iter().any(|byte| b"?#".contains(byte)) {
            url.possible_port(input, after)
        } else if relative {
            url.authority(&input[2..])
        } else {
            Some(url.tail(input))
        };
    }
    if after.is_empty() {
        url.scheme = Some(before);
        return Some(url);
    }
    if !after.starts_with(b"/") {
        let digits = digit_count(after);
        if digits <= 5 && (digits == after.len() || after.get(digits) == Some(&b'/')) {
            return url.possible_port(input, after);
        }
    }
    url.scheme = Some(before);
    match after.strip_prefix(b"//") {
        Some(rest) if before.eq_ignore_ascii_case(b"file") && rest.starts_with(b"/") => {
            Some(url.tail(if rest.get(2) == Some(&b':') {
                &rest[1..]
            } else {
                rest
            }))
        }
        Some(rest) => url.authority(rest),
        None => Some(url.tail(after)),
    }
}

impl<'a> Url<'a> {
    fn possible_port(mut self, input: &'a [u8], after: &[u8]) -> Option<Self> {
        let digits = digit_count(after);
        if digits > 0 && digits <= 5 && (digits == after.len() || after[digits] == b'/') {
            self.port = Some(parse_port(&after[..digits])?);
            self.authority(input.strip_prefix(b"//").unwrap_or(input))
        } else if after.is_empty() {
            None
        } else if let Some(host) = input.strip_prefix(b"//") {
            self.authority(host)
        } else {
            Some(self.tail(input))
        }
    }

    fn authority(mut self, input: &'a [u8]) -> Option<Self> {
        let end = input
            .iter()
            .position(|byte| b"/?#".contains(byte))
            .unwrap_or(input.len());
        let (mut host, tail) = input.split_at(end);
        if let Some(at) = host.iter().rposition(|&byte| byte == b'@') {
            let credentials = &host[..at];
            if let Some(colon) = credentials.iter().position(|&byte| byte == b':') {
                self.user = Some(&credentials[..colon]);
                self.pass = Some(&credentials[colon + 1..]);
            } else {
                self.user = Some(credentials);
            }
            host = &host[at + 1..];
        }
        if !(host.starts_with(b"[") && host.ends_with(b"]"))
            && let Some(colon) = host.iter().rposition(|&byte| byte == b':')
        {
            let port = &host[colon + 1..];
            if self.port.unwrap_or(0) == 0 && !port.is_empty() {
                if port.len() > 5 {
                    return None;
                }
                self.port = Some(parse_port(port)?);
            }
            host = &host[..colon];
        }
        if host.is_empty() {
            return None;
        }
        self.host = Some(host);
        Some(if tail.is_empty() {
            self
        } else {
            self.tail(tail)
        })
    }

    fn tail(mut self, input: &'a [u8]) -> Self {
        let mut path = input;
        if let Some(hash) = path.iter().position(|&byte| byte == b'#') {
            self.fragment = Some(&path[hash + 1..]);
            path = &path[..hash];
        }
        if let Some(question) = path.iter().position(|&byte| byte == b'?') {
            self.query = Some(&path[question + 1..]);
            path = &path[..question];
        }
        if !path.is_empty() || input.is_empty() {
            self.path = Some(path);
        }
        self
    }
}

fn digit_count(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count()
}

/// The authority uses the numeric prefix accepted by C's `strtol`.
fn parse_port(mut bytes: &[u8]) -> Option<i64> {
    while bytes
        .first()
        .is_some_and(|byte| matches!(byte, b' ' | 9..=13))
    {
        bytes = &bytes[1..];
    }
    let sign = if let Some(rest) = bytes.strip_prefix(b"-") {
        bytes = rest;
        -1
    } else {
        bytes = bytes.strip_prefix(b"+").unwrap_or(bytes);
        1
    };
    let digits = digit_count(bytes);
    let port = std::str::from_utf8(&bytes[..digits])
        .ok()?
        .parse::<i64>()
        .ok()?
        * sign;
    (0..=65535).contains(&port).then_some(port)
}
