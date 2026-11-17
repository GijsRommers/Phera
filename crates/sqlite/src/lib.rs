//! Shared SQLite engine: `new PDO('sqlite::memory:')` and `new SQLite3(':memory:')`.
//! Connections and live cursors stay on a dedicated thread; only owned data crosses channels.

mod backup;
mod connection;
mod cursor;
mod data;
mod lifetime;

pub use connection::Database;
pub use cursor::Statement;
pub use data::{Cell, Column, Description, Error, Execution};
pub use rusqlite::{version, version_number};

/// Even version queries cross the blocking boundary when called by a PHP worker.
pub async fn version_info() -> Result<(String, i32), Error> {
    tokio::task::spawn_blocking(|| (version().to_owned(), version_number()))
        .await
        .map_err(|_| Error::closed())
}
