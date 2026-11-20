//! PHP's HTTP client, exposed through `curl_init()` and `curl_multi_init()`.

mod classes;
mod connector;
mod constants;
mod cookies;
mod easy;
mod error;
mod info;
mod multi;
mod options;
mod share;
mod transfer;
mod upload;
mod version;

use phera_extension_api::{Extension, Registry};

pub struct Curl;

impl Extension for Curl {
    fn name(&self) -> &'static str {
        "curl"
    }

    fn register(&self, registry: &mut Registry) {
        constants::register(registry);
        classes::register(registry);
        easy::register(registry);
        multi::register(registry);
        share::register(registry);
    }
}
