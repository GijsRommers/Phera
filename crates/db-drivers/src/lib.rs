//! Shared async SQL connections used by PDO, mysqli, and pgsql.

pub mod connect_options;
pub mod drivers;
pub mod dsn;
pub mod errors;
pub mod metadata;
pub mod parameters;
pub mod placeholders;
pub mod pool;
