//! Async PDO for PHP: `$pdo = new PDO($dsn); $rows = $pdo->query($sql)->fetchAll()`.

mod attributes;
mod connect_options;
mod connection;
mod constants;
mod drivers;
mod dsn;
mod errors;
mod fetch;
mod fetch_all;
mod fetch_options;
mod metadata;
mod native_parameters;
mod parameters;
mod placeholders;
mod pool;
mod registration;
mod statement;
mod statement_class;
mod statement_execute;
mod stream_fetch;
mod transactions;

use phera_extension_api::{Extension, Registry};

pub struct Pdo;

impl Extension for Pdo {
    fn name(&self) -> &'static str {
        "PDO"
    }
    fn register(&self, registry: &mut Registry) {
        registration::register(registry);
    }
}
