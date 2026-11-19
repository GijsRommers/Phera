//! PHP's `ext/standard`: string, math, array and time functions.

use phera_extension_api::{Extension, Registry};

mod arguments;
mod arrays;
mod constants;
pub mod csv;
pub mod environment;
pub mod ini;
mod last_error;
mod locale;
mod math;
mod natural;
mod network;
mod offsets;
mod packing;
mod process;
mod process_info;
mod sleep;
mod strings;
mod time;
mod version_compare;

pub use url::query;
mod url;

pub struct Standard;

impl Extension for Standard {
    fn name(&self) -> &'static str {
        "standard"
    }

    fn register(&self, registry: &mut Registry) {
        // How a fork of a snapshot gets this extension's request state.
        registry.fork_state::<Option<crate::locale::state::RequestLocale>>(
            phera_extension_api::copy_state::<Option<crate::locale::state::RequestLocale>>,
        );
        registry.fork_state::<crate::process_info::memory::Peak>(phera_extension_api::fresh_state);
        registry.fork_state::<crate::strings::tokens::Tokenizer>(
            phera_extension_api::copy_state::<crate::strings::tokens::Tokenizer>,
        );
        registry.fork_state::<crate::environment::RequestEnvironment>(
            phera_extension_api::copy_state::<crate::environment::RequestEnvironment>,
        );
        registry.function(version_compare::ENTRY);
        strings::register(registry);
        registry.describe_function("strlen", "int", Some("Core"));
        url::register(registry);
        math::register(registry);
        arrays::register(registry);
        packing::register(registry);
        constants::register(registry);
        csv::register(registry);
        ini::register(registry);
        for entry in natural::FUNCTIONS.iter().chain(time::FUNCTIONS) {
            registry.function(*entry);
        }
        environment::register(registry);
        process::register(registry);
        last_error::register(registry);
        locale::register(registry);
        process_info::register(registry);
        sleep::register(registry);
        network::register(registry);
    }
}

pub use arrays::associative_sort;
/// Exposes PHP comparison traversal for adversarial comparator regression tests.
#[doc(hidden)]
pub use arrays::zend_sort::order as php_sort_order;

pub use strings::format::{Values as FormatValues, render as render_format};

pub use strings::base64::{decode as decode_base64, encode as encode_base64};
