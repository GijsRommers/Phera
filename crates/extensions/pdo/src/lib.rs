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
        "PDO"
    }
    fn register(&self, registry: &mut Registry) {
        registration::register(registry);
    }
}

/// The three database drivers are extensions of their own in PHP, each with
/// its connection class. Code asks for them by name:
///
/// ```php
/// if (extension_loaded('pdo_mysql')) { $options[Pdo\Mysql::ATTR_SSL_CA] = $ca; }
/// ```
///
/// Load them after [`Pdo`], whose class they extend.
pub struct PdoMysql;
pub struct PdoPgsql;
pub struct PdoSqlite;

impl Extension for PdoMysql {
    fn name(&self) -> &'static str {
        "pdo_mysql"
    }
    fn register(&self, registry: &mut Registry) {
        registration::register_mysql(registry);
    }
}

impl Extension for PdoPgsql {
    fn name(&self) -> &'static str {
        "pdo_pgsql"
    }
    fn register(&self, registry: &mut Registry) {
        registration::register_pgsql(registry);
    }
}

impl Extension for PdoSqlite {
    fn name(&self) -> &'static str {
        "pdo_sqlite"
    }
    fn register(&self, registry: &mut Registry) {
        registration::register_sqlite(registry);
    }
}
