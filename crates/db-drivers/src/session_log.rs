//! What a script changed about its database session, so that a fork of a
//! snapshot can set up a new session the same way.
//!
//! ```php
//! $pdo = new PDO('mysql:host=db;dbname=shop', 'app', 'secret');
//! $pdo->exec("SET SESSION sql_mode = 'TRADITIONAL'");   // noted, replayed
//! $pdo->query('SELECT 1');                               // not noted
//! Phera\snapshot();
//! ```
//!
//! A connection made before `Phera\snapshot()` cannot be shared by the
//! requests that fork the snapshot: each fork connects again on first use
//! and replays the noted statements, in order. The log also notes what a
//! replay cannot make again (a transaction, table locks, a temporary
//! table, an SQL-level prepared statement, a cursor or a `LISTEN`), and the
//! fork is refused while that state exists.
//!
//! Statements are recognised by their first words only. `SELECT ... INTO
//! @variable` or a function with session effects (`set_config()`,
//! `GET_LOCK()`) is not noted.

/// The session changes of one connection.
#[derive(Clone, Debug, Default)]
pub struct SessionLog {
    recording: bool,
    statements: Vec<Vec<u8>>,
    /// A transaction was started with a statement and not ended yet.
    transaction: bool,
    /// Table locks are held (`LOCK TABLES` without `UNLOCK TABLES` yet).
    locked: bool,
    /// Why the session holds state a replay cannot make again.
    lost: Option<&'static str>,
}

impl SessionLog {
    pub fn new(recording: bool) -> Self {
        Self {
            recording,
            ..Self::default()
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// The frozen log is replayed but the fork never adds to it.
    pub fn forked(&self) -> Self {
        Self {
            recording: false,
            ..self.clone()
        }
    }

    /// The statements to replay, in order.
    pub fn statements(&self) -> &[Vec<u8>] {
        &self.statements
    }

    /// Whether [`SessionLog::ran`] would note `sql`, so that a caller keeps
    /// a copy of only these.
    pub fn concerns(&self, sql: &[u8]) -> bool {
        self.recording
            && matches!(
                keywords(sql).next().as_deref(),
                Some(
                    "SET"
                        | "USE"
                        | "PRAGMA"
                        | "ATTACH"
                        | "DETACH"
                        | "LOCK"
                        | "UNLOCK"
                        | "CREATE"
                        | "PREPARE"
                        | "DECLARE"
                        | "LISTEN"
                        | "BEGIN"
                        | "START"
                        | "COMMIT"
                        | "ROLLBACK"
                        | "END"
                )
            )
    }

    /// Notes a statement the script ran successfully.
    pub fn ran(&mut self, sql: &[u8]) {
        if !self.recording {
            return;
        }
        let mut words = keywords(sql);
        let first = words.next();
        let second = words.next();
        match (first.as_deref(), second.as_deref()) {
            (Some("SET" | "USE" | "PRAGMA" | "ATTACH" | "DETACH"), _) => self.record(sql.to_vec()),
            (Some("LOCK"), _) => self.locked = true,
            (Some("UNLOCK"), _) => self.locked = false,
            (Some("CREATE"), Some("TEMPORARY" | "TEMP")) => {
                self.lost = Some("it created a temporary table");
            }
            (Some("PREPARE"), _) => self.lost = Some("it prepared an SQL statement with PREPARE"),
            (Some("DECLARE"), _) => self.lost = Some("it declared a cursor"),
            (Some("LISTEN"), _) => self.lost = Some("it listens for notifications"),
            (Some("BEGIN" | "START"), _) => self.transaction = true,
            (Some("ROLLBACK"), Some("TO")) => {}
            (Some("COMMIT" | "ROLLBACK" | "END"), _) => self.transaction = false,
            _ => {}
        }
    }

    /// Notes statements run together (`multi_query()`), which are not
    /// replayed.
    pub fn ran_several(&mut self) {
        if self.recording {
            self.lost = Some("it ran statements with multi_query()");
        }
    }

    /// Notes a statement that an API call ran for the script, such as the
    /// `SET NAMES` of `mysqli::set_charset()`.
    pub fn record(&mut self, statement: Vec<u8>) {
        if !self.recording {
            return;
        }
        // A statement run again replaces the earlier one, so the list stays
        // as short as the distinct settings.
        self.statements.retain(|earlier| *earlier != statement);
        self.statements.push(statement);
    }

    /// Why a new session with the replayed statements would differ from
    /// this one, if it would.
    pub fn unreplayable(&self) -> Option<&'static str> {
        if self.transaction {
            Some("a transaction is open on it")
        } else if self.locked {
            Some("it holds table locks")
        } else {
            self.lost
        }
    }
}

/// The first words of `sql`, upper-cased, after comments and white space.
fn keywords(sql: &[u8]) -> impl Iterator<Item = String> + '_ {
    let mut rest = sql;
    std::iter::from_fn(move || {
        rest = skip_comments(rest);
        let length = rest
            .iter()
            .position(|byte| !byte.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        if length == 0 {
            return None;
        }
        let (word, after) = rest.split_at(length);
        rest = after;
        Some(String::from_utf8_lossy(word).to_ascii_uppercase())
    })
}

fn skip_comments(mut sql: &[u8]) -> &[u8] {
    loop {
        sql = sql.trim_ascii_start();
        if let Some(rest) = sql.strip_prefix(b"/*") {
            let end = rest.windows(2).position(|pair| pair == b"*/");
            sql = end.map_or(&[], |end| &rest[end + 2..]);
        } else if sql.starts_with(b"#") || sql.starts_with(b"--") {
            let end = sql.iter().position(|byte| *byte == b'\n');
            sql = end.map_or(&[], |end| &sql[end + 1..]);
        } else {
            return sql;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SessionLog;

    #[test]
    fn notes_session_statements_once_each_in_order() {
        let mut log = SessionLog::new(true);
        log.ran(b"SET NAMES utf8mb4");
        log.ran(b"  /* wp */ set session sql_mode = ''");
        log.ran(b"SELECT 1");
        log.ran(b"SET NAMES utf8mb4");
        assert_eq!(
            log.statements(),
            [
                b"  /* wp */ set session sql_mode = ''".to_vec(),
                b"SET NAMES utf8mb4".to_vec()
            ]
        );
        assert!(log.unreplayable().is_none());
    }

    #[test]
    fn notes_state_a_replay_cannot_make() {
        let mut log = SessionLog::new(true);
        log.ran(b"LOCK TABLES t WRITE");
        assert!(log.unreplayable().is_some());
        log.ran(b"-- done\nUNLOCK TABLES");
        assert!(log.unreplayable().is_none());
        log.ran(b"BEGIN");
        log.ran(b"ROLLBACK TO SAVEPOINT a");
        assert!(log.unreplayable().is_some());
        log.ran(b"commit");
        assert!(log.unreplayable().is_none());
        log.ran(b"create temp table t (id int)");
        assert!(log.unreplayable().is_some());
    }

    #[test]
    fn concerns_only_what_it_notes() {
        let log = SessionLog::new(true);
        assert!(log.concerns(b"\n# x\nPRAGMA foreign_keys = ON"));
        assert!(!log.concerns(b"SELECT 1"));
        assert!(!log.concerns(b"INSERT INTO t VALUES (1)"));
        assert!(!log.forked().concerns(b"SET TIME ZONE 'UTC'"));
    }
}
