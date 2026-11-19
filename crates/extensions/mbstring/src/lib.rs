//! PHP's mbstring and iconv extensions: strings as characters in a named
//! encoding instead of as bytes.
//!
//! ```php
//! strlen('Straße');              // 7
//! mb_strlen('Straße');           // 6
//! mb_strtoupper('Straße');       // "STRASSE"
//! mb_convert_encoding('é', 'ISO-8859-1', 'UTF-8');  // "\xE9"
//! iconv('UTF-8', 'ASCII//IGNORE', 'aéb');           // "ab"
//! ```
//!
//! [`encoding`] names the encodings and [`codec`] converts them; [`unicode`]
//! has PHP's case and width tables. Each other module registers the PHP
//! functions it implements. The README lists what is not done.

mod casing;
mod codec;
mod conversion;
mod cut;
mod encoding;
mod entities;
mod iconv;
mod ini;
mod kana;
mod mime;
mod names;
mod query;
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
        // How a fork of a snapshot gets this extension's request state.
        registry.fork_state::<crate::state::Settings>(
            phera_extension_api::copy_state::<crate::state::Settings>,
        );
        registry.fork_state::<crate::iconv::settings::Settings>(
            phera_extension_api::copy_state::<crate::iconv::settings::Settings>,
        );
        text::register(registry);
        casing::register(registry);
        search::register(registry);
        trim::register(registry);
        cut::register(registry);
        conversion::register(registry);
        names::register(registry);
        settings::register(registry);
        ini::register(registry);
        entities::register(registry);
        kana::register(registry);
        mime::register(registry);
        query::register(registry);
        iconv::register(registry);
        for case in unicode::Case::ALL {
            registry.constant(&format!("MB_CASE_{}", case.constant_name()), case as i64);
        }
    }
}

/// PHP's string conversion for values the argument checks do not reach,
/// such as array elements; diagnostics are not reported.
fn string_of(value: &phera_extension_api::Value) -> String {
    let mut diagnostics = phera_extension_api::value::CollectDiagnostics::default();
    phera_extension_api::value::to_php_string(value, &mut diagnostics)
        .map(|s| s.to_string_lossy())
        .unwrap_or_default()
}
