//! URL and query helpers, for example `parse_url('/api?x=1', PHP_URL_PATH)`.

mod build;
mod parse;
pub mod query;

use phera_extension_api::Registry;

pub(super) fn register(registry: &mut Registry) {
    for entry in [parse::ENTRY, query::ENTRY, build::ENTRY] {
        registry.function(entry);
    }
    for (index, (name, _)) in parse::COMPONENTS.iter().enumerate() {
        registry.constant(name, index as i64);
    }
    registry.constant("PHP_QUERY_RFC1738", 1_i64);
    registry.constant("PHP_QUERY_RFC3986", 2_i64);
}
