//! Multibyte text operations: `mb_strlen("é") === 1`.

mod casing;
mod codec;
mod conversion;
mod cut;
mod encoding;
mod entities;
mod iconv;
mod iconv_decode;
mod iconv_mime;
mod kana;
mod mime;
mod names;
mod query;
mod register_extra;
mod search;
mod settings;
mod state;
mod text;
mod trim;
mod unicode;
mod walk;

use phera_extension_api::{Extension, Param, ParamType, Registry};

/// `string $string`, the first parameter of most functions.
const STRING: Param = Param::new("string", ParamType::String);
/// `?string $encoding = null`, the last parameter of most functions: the
/// internal encoding when left out.
const ENCODING: Param = Param::new("encoding", ParamType::String)
    .nullable()
    .optional();

/// PHP's request-local multibyte string extension.
pub struct Mbstring;

impl Extension for Mbstring {
    fn name(&self) -> &'static str {
        "mbstring"
    }

    fn register(&self, registry: &mut Registry) {
        text::register(registry);
        casing::register(registry);
        search::register(registry);
        trim::register(registry);
        cut::register(registry);
        conversion::register(registry);
        names::register(registry);
        settings::register(registry);
        entities::register(registry);
        kana::register(registry);
        mime::register(registry);
        query::register(registry);
        register_extra::register(registry);
        registry.constant("ICONV_MIME_DECODE_STRICT", 1i64);
        registry.constant("ICONV_MIME_DECODE_CONTINUE_ON_ERROR", 2i64);
        for case in unicode::Case::ALL {
            registry.constant(&format!("MB_CASE_{}", case.constant_name()), case as i64);
        }
    }
}

fn string_of(value: &phera_extension_api::Value) -> String {
    let mut diagnostics = phera_extension_api::value::CollectDiagnostics::default();
    phera_extension_api::value::to_php_string(value, &mut diagnostics)
        .map(|s| s.to_string_lossy())
        .unwrap_or_default()
}
