//! SQLite through `phera-sqlite`:
//!
//! ```php
//! $pdo = new PDO('sqlite::memory:');
//! $pdo->exec('CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)');
//! $statement = $pdo->query('SELECT * FROM t');
//! while ($row = $statement->fetch()) { ... } // each fetch steps the cursor
//! ```
//!
//! Two decisions make SQLite differ from the server drivers:
//!
//! - **Its calls block, so they run on a thread of their own.** SQLite has no
//!   network protocol to wait on asynchronously: every call does file I/O
//!   and may wait for a lock. Each database therefore lives on a dedicated
//!   thread (see `phera_sqlite::Database`), and the PHP worker awaits the
//!   answer like any other I/O. A statement keeps a live cursor there, so
//!   `fetch()` steps it one row at a time instead of buffering the result,
//!   which is why execute and fetch use the statement handle directly
//!   (`statement` and `fetch` here) rather than the pooled lease.
//! - **Its handles are never pooled.** A reset cannot make a SQLite handle
//!   clean for the next `new PDO`: an in-memory database is the data itself,
//!   and a file handle keeps attached databases, temporary tables and
//!   pragmas. `crate::pool::acquire` therefore always opens one unpooled.

pub mod bindings;
pub mod fetch;
pub mod methods;
pub mod statement;

use phera_sqlite::Database;

use super::RawRows;
use crate::dsn::Dsn;
use crate::errors::DbError;

/// SQLite's primary result codes that PDO maps to its own SQLSTATEs.
const SQLITE_CANTOPEN: i32 = 14;
const SQLITE_TOOBIG: i32 = 18;
const SQLITE_CONSTRAINT: i32 = 19;

/// An extended result code is the primary code in its low byte.
const PRIMARY_CODE: i32 = 0xff;

/// `SQLITE_OPEN_URI`: pdo_sqlite always lets a file name be a `file:` URI.
const OPEN_URI: i32 = 0x40;

pub struct Sqlite {
    pub database: Database,
}

impl Sqlite {
    /// The constructor puts the options' open flags and timeout into the
    /// DSN's fields before it gets here.
    pub async fn connect(dsn: &Dsn) -> Result<Self, DbError> {
        let flags = dsn.get("flags", "6").parse::<i32>().unwrap_or(6);
        let timeout = dsn.get("timeout", "60").parse::<i64>().unwrap_or(60);
        let database = Database::open(
            dsn.get("filename", "").into(),
            flags | OPEN_URI,
            timeout.max(0).saturating_mul(1000) as u64,
        )
        .await
        .map_err(error)?;
        Ok(Self { database })
    }
}

impl Sqlite {
    /// SQL text, which may hold several statements: `exec()`, `BEGIN` ...
    pub async fn run_unprepared(&self, sql: &str) -> Result<RawRows, DbError> {
        self.database
            .exec(sql.into())
            .await
            .map(RawRows::Affected)
            .map_err(error)
    }

    /// `ATTR_TIMEOUT`: how long SQLite waits for a lock.
    pub async fn set_timeout(&self, milliseconds: u64) -> Result<(), DbError> {
        self.database.timeout(milliseconds).await.map_err(error)
    }
}

/// PDO's error for a SQLite failure: the SQLSTATE comes from the primary
/// result code, the driver code is the result code itself.
pub fn error(error: phera_sqlite::Error) -> DbError {
    let primary = error.code & PRIMARY_CODE;
    DbError {
        state: sqlstate(primary).into(),
        code: Some(i64::from(error.code)),
        detail: if primary == SQLITE_CANTOPEN {
            "unable to open database file".into()
        } else {
            error.message
        },
    }
}

/// Without `ATTR_EXTENDED_RESULT_CODES` PDO reports the primary code, and
/// PHP picks the SQLSTATE from the code it reports, not from the primary
/// one: an extended constraint code gives HY000.
pub fn normalize(error: &mut DbError, extended: bool) {
    if !extended {
        error.code = error.code.map(|code| code & i64::from(PRIMARY_CODE));
    }
    if let Some(code) = error.code {
        error.state = sqlstate(i32::try_from(code).unwrap_or(0)).into();
    }
}

fn sqlstate(code: i32) -> &'static str {
    match code {
        SQLITE_CONSTRAINT => "23000",
        SQLITE_TOOBIG => "22001",
        _ => "HY000",
    }
}

/// `$pdo->quote()`: pdo_sqlite stops at the first NUL byte and doubles
/// single quotes, whatever the encoding.
pub fn quote(bytes: &[u8]) -> Vec<u8> {
    let mut quoted = vec![b'\''];
    for &byte in bytes.iter().take_while(|byte| **byte != 0) {
        quoted.push(byte);
        if byte == b'\'' {
            quoted.push(byte);
        }
    }
    quoted.push(b'\'');
    quoted
}
