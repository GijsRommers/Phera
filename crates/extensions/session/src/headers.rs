//! `session_start()` can append a cookie and replace the HTTP cache headers.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::DateTime;
use phera_ext_http::{Sapi, module_cookie, module_header, sapi};
use phera_ext_streams::file_io::blocking;
use phera_extension_api::{Context, NativeError};

use crate::{
    id::c_string,
    settings,
    state::{after_output, state},
};

const EXPIRED: &[u8] = b"Expires: Thu, 19 Nov 1981 08:52:00 GMT";

pub fn cookie(context: &mut dyn Context, function: &str) -> bool {
    if context.output_origin().is_some() {
        let message = format!(
            "{function}(): Session cookie cannot be sent after headers have already been sent"
        );
        context.warning(after_output(context, &message));
        return false;
    }
    let secure = settings::boolean(context, "cookie_secure");
    let partitioned = settings::boolean(context, "cookie_partitioned");
    if partitioned && !secure {
        context.warning(format!(
            "{function}(): Partitioned session cookie cannot be used without also configuring it as secure"
        ));
        return false;
    }
    if matches!(sapi(context), Sapi::Cli) {
        return true;
    }
    let name = settings::text(context, "name");
    let mut line = name.as_bytes().to_vec();
    line.push(b'=');
    if let Some(id) = &state(context).id {
        url_encode(&mut line, id.as_bytes());
    }
    let lifetime = settings::integer(context, "cookie_lifetime");
    if lifetime > 0
        && let Some(expires) = current_time().checked_add(lifetime).and_then(http_date)
    {
        line.extend_from_slice(format!("; expires={expires}; Max-Age={lifetime}").as_bytes());
    }
    for (attribute, setting) in [
        (b"; path=".as_slice(), "cookie_path"),
        (b"; domain=", "cookie_domain"),
    ] {
        append(
            &mut line,
            attribute,
            settings::text(context, setting).as_bytes(),
        );
    }
    if secure {
        line.extend_from_slice(b"; secure");
    }
    if partitioned {
        line.extend_from_slice(b"; Partitioned");
    }
    if settings::boolean(context, "cookie_httponly") {
        line.extend_from_slice(b"; HttpOnly");
    }
    append(
        &mut line,
        b"; SameSite=",
        settings::text(context, "cookie_samesite").as_bytes(),
    );
    module_cookie(context, name.as_bytes(), &line)
}

pub async fn cache(context: &mut dyn Context, function: &str) -> Result<bool, NativeError> {
    let limiter = settings::text(context, "cache_limiter");
    let limiter = c_string(limiter.as_bytes()).to_ascii_lowercase();
    if limiter.is_empty() {
        return Ok(true);
    }
    if context.output_origin().is_some() {
        let message = format!(
            "{function}(): Session cache limiter cannot be sent after headers have already been sent"
        );
        context.warning(after_output(context, &message));
        return Ok(false);
    }
    if matches!(sapi(context), Sapi::Cli) {
        return Ok(true);
    }
    let seconds = settings::integer(context, "cache_expire").wrapping_mul(60);
    let policy = match limiter.as_slice() {
        b"nocache" => {
            module_header(context, EXPIRED);
            module_header(
                context,
                b"Cache-Control: no-store, no-cache, must-revalidate",
            );
            module_header(context, b"Pragma: no-cache");
            return Ok(true);
        }
        b"private" => {
            module_header(context, EXPIRED);
            "private"
        }
        b"private_no_expire" => "private",
        b"public" => {
            let date = http_date(current_time().wrapping_add(seconds)).unwrap_or_default();
            module_header(context, format!("Expires: {date}").as_bytes());
            "public"
        }
        _ => return Ok(true),
    };
    module_header(
        context,
        format!("Cache-Control: {policy}, max-age={seconds}").as_bytes(),
    );
    if let Some(path) = context.script_path() {
        let modified = blocking(move || {
            let time = std::fs::metadata(path).ok()?.modified().ok()?;
            time.duration_since(UNIX_EPOCH)
                .ok()
                .and_then(|time| http_date(time.as_secs() as i64))
        })
        .await?;
        if let Some(date) = modified {
            module_header(context, format!("Last-Modified: {date}").as_bytes());
        }
    }
    Ok(true)
}

fn append(line: &mut Vec<u8>, attribute: &[u8], value: &[u8]) {
    if !value.is_empty() {
        line.extend_from_slice(attribute);
        line.extend_from_slice(value);
    }
}

fn url_encode(line: &mut Vec<u8>, value: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in value {
        match byte {
            b' ' => line.push(b'+'),
            b'-' | b'_' | b'.' => line.push(byte),
            byte if byte.is_ascii_alphanumeric() => line.push(byte),
            _ => line.extend_from_slice(&[
                b'%',
                HEX[usize::from(byte >> 4)],
                HEX[usize::from(byte & 15)],
            ]),
        }
    }
}

fn current_time() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs() as i64)
}

fn http_date(timestamp: i64) -> Option<String> {
    DateTime::from_timestamp(timestamp, 0)
        .map(|date| date.format("%a, %d %b %Y %H:%M:%S GMT").to_string())
}
