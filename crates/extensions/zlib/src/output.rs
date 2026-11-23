//! `ob_start('ob_gzhandler')` negotiates gzip or deflate on an HTTP request.

use phera_ext_http::ResponseHead;
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, Value, args};

use crate::deflate::{Encoder, Flush};
use crate::format::Encoding;

const START: i64 = 1;
const CLEAN: i64 = 2;
const FLUSH: i64 = 4;
const FINAL: i64 = 8;

#[derive(Default)]
pub(crate) struct Output {
    encoding: Option<Encoding>,
    encoder: Option<Encoder>,
}

fn choose(context: &mut dyn Context) -> Option<Encoding> {
    if let Some(encoding) = context.state().get_or_default::<Output>().encoding {
        return Some(encoding);
    }
    let Value::Array(server) = context.request_global("_SERVER")? else {
        return None;
    };
    let header = server.get(&ArrayKey::str("HTTP_ACCEPT_ENCODING"))?;
    let header = phera_extension_api::value::to_php_string(header, context.diagnostics()).ok()?;
    let bytes = header.as_bytes().split(|byte| *byte == 0).next()?;
    let encoding = if bytes.windows(4).any(|part| part == b"gzip") {
        Encoding::Gzip
    } else if bytes.windows(7).any(|part| part == b"deflate") {
        Encoding::Zlib
    } else {
        return None;
    };
    context.state().get_or_default::<Output>().encoding = Some(encoding);
    Some(encoding)
}

fn name(encoding: Encoding) -> &'static str {
    if encoding == Encoding::Gzip {
        "gzip"
    } else {
        "deflate"
    }
}

pub(crate) fn coding_type(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(context
        .state()
        .get_or_default::<Output>()
        .encoding
        .map_or(Value::Bool(false), |encoding| Value::from(name(encoding))))
}

pub(crate) fn gzhandler(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let Some(encoding) = choose(context) else {
        return Ok(Value::Bool(false));
    };
    let flags = args::int(values, 1).unwrap_or(0);
    if flags & START != 0 {
        context
            .state()
            .get_or_default::<ResponseHead>()
            .set_content_coding(name(encoding));
    }
    let setting = context
        .ini_get(b"zlib.output_compression_level")
        .map_or(-1, |setting| {
            phera_extension_api::ini::integer(setting.as_bytes())
        });
    if !(-1..=9).contains(&setting) {
        return Ok(Value::Bool(false));
    }
    let level = if setting == -1 { 6 } else { setting as u32 };
    let state = context.state().get_or_default::<Output>();
    if flags & CLEAN != 0 {
        state.encoder = (flags & FINAL == 0).then(|| Encoder::new(encoding, level, 15, None));
        return Ok(Value::from(""));
    }
    if flags & START != 0 {
        state.encoder = Some(Encoder::new(encoding, level, 15, None));
    }
    let Some(encoder) = state.encoder.as_mut() else {
        return Ok(Value::Bool(false));
    };
    let flush = if flags & FINAL != 0 {
        Flush::Finish
    } else if flags & FLUSH != 0 {
        Flush::Full
    } else {
        Flush::Sync
    };
    let result = encoder.add(args::bytes(values, 0), flush);
    if flags & FINAL != 0 || result.is_err() {
        state.encoder = None;
    }
    Ok(result.map_or(Value::Bool(false), Value::from))
}

pub(crate) fn update_compression(
    context: &mut dyn Context,
    name: &str,
    _: Option<&[u8]>,
    function: &str,
) -> Result<bool, NativeError> {
    if context.output_origin().is_some() {
        context.warning(format!(
            "{function}(): Cannot change {name} - headers already sent"
        ));
        return Ok(false);
    }
    Ok(true)
}
