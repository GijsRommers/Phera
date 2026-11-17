//! Multibyte text operations: `mb_strlen("é") === 1`.
mod boundary;
mod cjk;
mod conversion;
mod entities;
mod gb18030;
mod iconv;
mod iconv_decode;
mod iconv_mime;
mod info;
mod iso2022;
mod mime;
mod pseudo;
mod query;
mod register_extra;

mod decode;
mod encode;
mod encoding;
mod kana;
mod layout;
mod register_search;
mod register_settings;
mod register_text;
mod search;
mod search_bytes;
mod settings;
mod state;
mod text;
mod transfer;
mod unicode;
mod walk;
use phera_extension_api::{Extension, Registry};

/// PHP's request-local multibyte string extension.
pub struct Mbstring;

impl Extension for Mbstring {
    fn name(&self) -> &'static str {
        "mbstring"
    }

    fn register(&self, registry: &mut Registry) {
        register_text::register(registry);
        register_search::register(registry);
        register_extra::register(registry);
        registry.constant("ICONV_MIME_DECODE_STRICT", 1i64);
        registry.constant("ICONV_MIME_DECODE_CONTINUE_ON_ERROR", 2i64);
        register_settings::register(registry);
        for (i, name) in [
            "UPPER",
            "LOWER",
            "TITLE",
            "FOLD",
            "UPPER_SIMPLE",
            "LOWER_SIMPLE",
            "TITLE_SIMPLE",
            "FOLD_SIMPLE",
        ]
        .iter()
        .enumerate()
        {
            registry.constant(&format!("MB_CASE_{name}"), i as i64);
        }
    }
}

fn encoding_name(value: &phera_extension_api::Value) -> String {
    let mut diagnostics = phera_extension_api::value::CollectDiagnostics::default();
    phera_extension_api::value::to_php_string(value, &mut diagnostics)
        .map(|s| s.to_string_lossy())
        .unwrap_or_default()
}
