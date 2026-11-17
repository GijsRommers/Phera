//! `sqlite:` uses a private database thread and never enters the connection pool.

use crate::dsn::Dsn;
use crate::errors::DbError;
use phera_sqlite::Database;

pub struct Sqlite {
    pub database: Database,
}

impl Sqlite {
    pub async fn connect(dsn: &Dsn) -> Result<Self, DbError> {
        let flags = dsn.get("flags", "6").parse::<i32>().unwrap_or(6);
        let timeout = dsn.get("timeout", "60").parse::<i64>().unwrap_or(60);
        let database = Database::open(
            dsn.get("filename", "").into(),
            flags | 64,
            timeout.max(0).saturating_mul(1000) as u64,
        )
        .await
        .map_err(error)?;
        Ok(Self { database })
    }
}

pub fn error(error: phera_sqlite::Error) -> DbError {
    let primary = error.code & 255;
    DbError {
        state: match primary {
            19 => "23000",
            18 => "22001",
            _ => "HY000",
        }
        .into(),
        code: Some(i64::from(error.code)),
        detail: if primary == 14 {
            "unable to open database file".into()
        } else {
            error.message
        },
    }
}

pub fn normalize(error: &mut DbError, extended: bool) {
    if !extended {
        error.code = error.code.map(|code| code & 255);
    }
    // PHP switches on the reported code, not its primary portion, for SQLSTATE.
    if let Some(code) = error.code {
        error.state = match code {
            19 => "23000",
            18 => "22001",
            _ => "HY000",
        }
        .into();
    }
}
