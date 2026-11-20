//! Asynchronous MySQL for `mysqli_query($connection, $sql)` and `$db->query($sql)`.

mod connection;
mod constants;
mod errors;
mod options;
mod registration;
mod result;
mod statements;

use phera_extension_api::{Extension, Registry};

pub struct Mysqli;

impl Extension for Mysqli {
    fn name(&self) -> &'static str {
        "mysqli"
    }

    fn php_module(&self) -> Option<&'static str> {
        Some("mysqli")
    }

    fn register(&self, registry: &mut Registry) {
        constants::register(registry);
        registration::register(registry);
    }
}
