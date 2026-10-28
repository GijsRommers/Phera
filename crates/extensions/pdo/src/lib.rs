//! PHP's `ext/pdo`. Registers nothing yet: PDO is built from classes
//! (`PDO`, `PDOStatement`, `PDOException`), which the extension API can't
//! express so far.

use phera_extension_api::{Extension, Registry};

pub struct Pdo;

impl Extension for Pdo {
    fn name(&self) -> &'static str {
        "pdo"
    }

    fn register(&self, _registry: &mut Registry) {}
}
