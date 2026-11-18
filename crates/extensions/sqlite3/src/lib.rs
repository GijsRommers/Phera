//! Native SQLite classes: `$db = new SQLite3(':memory:'); $db->querySingle('SELECT 42');`.

mod backup;
mod connection;
mod constants;
mod diagnostics;
mod query;
mod registration;
mod result;
mod statement;
mod statement_info;
mod utility;

use phera_extension_api::{Extension, Registry};
pub struct Sqlite3;
impl Extension for Sqlite3 {
    fn name(&self) -> &'static str {
        "sqlite3"
    }
    fn register(&self, registry: &mut Registry) {
        registration::register(registry);
    }
}
