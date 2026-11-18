//! Async PDO for PHP: `$pdo = new PDO($dsn); $rows = $pdo->query($sql)->fetchAll()`.

mod attributes;
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
mod statement_execute;
mod transactions;

use phera_extension_api::{Extension, Registry};

pub struct Pdo;

impl Extension for Pdo {
    fn name(&self) -> &'static str {
        "pdo"
    }
    fn register(&self, registry: &mut Registry) {
        registration::register(registry);
    }
}
