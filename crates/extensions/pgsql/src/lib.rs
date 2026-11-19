//! PostgreSQL functions such as `pg_query($connection, $sql)`.

mod connection;
mod constants;
mod copy;
mod dml;
mod escape;
mod information;
mod query;
mod registration;
mod result;

use phera_extension_api::{Extension, Registry};

pub struct Pgsql;

impl Extension for Pgsql {
    fn name(&self) -> &'static str {
        "pgsql"
    }

    fn php_module(&self) -> Option<&'static str> {
        Some("pgsql")
    }

    fn register(&self, registry: &mut Registry) {
        constants::register(registry);
        registration::register(registry);
    }
}
