//! `$source->backup($destination)` copies SQLite pages, including in-memory databases.
//! Snapshot files are created and removed only on database threads.

use crate::{Database, Error};

struct Snapshot(Option<tempfile::NamedTempFile>);

impl Snapshot {
    fn path(&self) -> &std::path::Path {
        self.0.as_ref().expect("live snapshot").path()
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        if let Some(file) = self.0.take() {
            // Cancellation can drop the transfer on a PHP worker. Unlink off that worker.
            tokio::task::spawn_blocking(move || drop(file));
        }
    }
}

impl Database {
    pub async fn backup(
        &self,
        destination: &Self,
        source: String,
        target: String,
    ) -> Result<(), Error> {
        if self.same_connection(destination) {
            return Err(Error {
                code: 1,
                message: "source and destination must be distinct".into(),
            });
        }
        let snapshot = self
            .call(move |connection| {
                let snapshot = tempfile::NamedTempFile::new().map_err(|e| Error {
                    code: 14,
                    message: e.to_string(),
                })?;
                connection.backup(
                    source.as_str(),
                    snapshot.path(),
                    rusqlite::backup::NO_PROGRESS,
                )?;
                Ok(Snapshot(Some(snapshot)))
            })
            .await?;
        destination
            .exclusive(move |connection| {
                connection.restore(
                    target.as_str(),
                    snapshot.path(),
                    rusqlite::backup::NO_PROGRESS,
                )?;
                Ok(())
            })
            .await
    }
}
