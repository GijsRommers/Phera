//! MySQL uses binary prepared statements, or text queries for `ATTR_EMULATE_PREPARES`.

use std::sync::Arc;

use mysql_async::prelude::{Protocol, Queryable};
use mysql_async::{Conn, Params, QueryResult, Statement};

mod connect;
mod information;
mod native;
pub use native::{Outcome, Reply, identifier};
mod stream;
mod tls;
pub mod values;

#[cfg(test)]
mod auth_tests;

use super::{Execution, Prepared, RowBuffer, Rows};
use crate::connect_options::MysqlOptions;
use crate::errors::DbError;
use crate::parameters::{BoundValue, push_literal, push_quote, quote};
use crate::placeholders::{Sql, Syntax};
use values::parameter;

pub struct Mysql {
    connection: Option<Conn>,
    active: Option<stream::Active>,
    pub version: String,
    last_insert_id: u64,
    pending_exec_rows: bool,
    /// Set whenever the request ran `select_db` or SQL containing `USE`, so
    /// clean queries skip the redundant `USE` statement after `COM_RESET_CONNECTION`.
    db_changed: bool,
    /// Set whenever the connection may have modified MySQL session state
    /// (transactions, SET, USE, user vars, temp tables, locks, AUTO_INCREMENT
    /// LAST_INSERT_ID, errors, or non-allowlisted SQL). Clean DML-only requests
    /// skip `COM_RESET_CONNECTION` + `SET NAMES` and keep `stmt_cache` warm.
    session_dirty: bool,
    /// Pre-built multi-statement post-reset SQL including `USE` + `SET NAMES` + options.
    reset_sql_with_db: String,
    /// Pre-built multi-statement post-reset SQL when the current database did not change.
    reset_sql_keep_db: String,
    charset: Option<String>,
    database: String,
    options: MysqlOptions,
    host: String,
    reusable: bool,
    unix_socket: bool,
}

impl Mysql {
    pub async fn close(mut self) {
        self.abort_stream().await;
        if let Some(connection) = self.connection.take() {
            let _ = connection.disconnect().await;
        }
    }

    /// Whether returning this connection to the pool requires a network
    /// `COM_RESET_CONNECTION` round-trip to restore session isolation.
    pub fn needs_reset(&self) -> bool {
        self.session_dirty
            || self.db_changed
            || self.pending_exec_rows
            || self.active.is_some()
            || self.transaction_state() == Some(true)
            || self.options.init.is_some()
            || !self.options.autocommit
            || (self.options.restore_empty_database && self.database.is_empty())
    }

    /// Resets per-request local driver state for a verified-clean connection
    /// without issuing any network round-trip.
    pub fn reset_clean_local(&mut self) {
        self.last_insert_id = 0;
        self.pending_exec_rows = false;
    }

    pub async fn reset(&mut self) -> Result<(), DbError> {
        if !self.needs_reset() {
            // Clean DML-only session: a 5-byte COM_PING verifies the socket was
            // not killed while borrowed, without running COM_RESET_CONNECTION,
            // re-sending SET NAMES, or flushing mysql_async's stmt_cache.
            self.connection()?.ping().await.map_err(error)?;
            self.reset_clean_local();
            return Ok(());
        }
        self.finish_stream().await;
        let connection = self.connection.as_mut().ok_or_else(Self::closed)?;
        // COM_CHANGE_USER also restores a connection opened without a database.
        let empty_database = self.options.restore_empty_database && self.database.is_empty();
        if empty_database || !connection.reset().await.map_err(error)? {
            connection
                .change_user(Default::default())
                .await
                .map_err(error)?;
        }
        // Run post-reset session restoration (`USE`, `SET NAMES`, `sql_mode`,
        // `autocommit=0`) in a single multi-statement round-trip instead of
        // 2-4 sequential network hops, skipping `USE` when unchanged.
        let post_reset = if self.db_changed {
            &self.reset_sql_with_db
        } else {
            &self.reset_sql_keep_db
        };
        if !post_reset.is_empty() {
            connection.query_drop(post_reset).await.map_err(error)?;
        }
        if let Some(init) = &self.options.init {
            if !self.options.multiple {
                require_single_statement(connection, init).await?;
            }
            connection.query_drop(init).await.map_err(error)?;
        }
        self.last_insert_id = 0;
        self.pending_exec_rows = false;
        self.db_changed = false;
        self.session_dirty = false;
        Ok(())
    }

    pub fn is_broken(&self) -> bool {
        self.active.is_none() && self.connection.as_ref().is_none_or(Conn::is_disconnected)
    }

    pub async fn health(&mut self) -> Result<(), DbError> {
        self.connection()?.ping().await.map_err(error)
    }

    pub async fn set_autocommit(&mut self, enabled: bool) -> Result<(), DbError> {
        self.check_pending().await?;
        self.session_dirty = true;
        self.connection()?
            .query_drop(if enabled {
                "SET autocommit=1"
            } else {
                "SET autocommit=0"
            })
            .await
            .map_err(error)
    }

    pub async fn prepare(&mut self, sql: &Sql) -> Result<Statement, DbError> {
        self.check_pending().await?;
        if !self.session_dirty && !is_clean_dml_sql(sql.source.as_bytes()) {
            self.session_dirty = true;
        }
        let rendered = sql.render(|_| "?".into());
        match self.connection()?.prep(rendered).await {
            Ok(stmt) => Ok(stmt),
            Err(cause) => {
                self.session_dirty = true;
                Err(error(cause))
            }
        }
    }

    pub async fn execute(
        &mut self,
        sql: &Sql,
        prepared: &Prepared,
        values: &[BoundValue],
        options: &Execution,
    ) -> Result<Rows, DbError> {
        self.check_pending().await?;
        match prepared {
            Prepared::Mysql(statement) => {
                if !self.session_dirty && !is_clean_dml_sql(sql.source.as_bytes()) {
                    self.session_dirty = true;
                }
                let params = Params::Positional(values.iter().map(parameter).collect());
                if let Some(owner) = &options.mysql_stream {
                    self.session_dirty = true;
                    return self
                        .stream(stream::Query::Binary(statement.clone(), params), owner)
                        .await?
                        .rows
                        .into_php();
                }
                let result = match self.connection()?.exec_iter(statement, params).await {
                    Ok(result) => result,
                    Err(cause) => {
                        self.session_dirty = true;
                        return Err(error(cause));
                    }
                };
                let (rows, insert_id) = match buffered(result).await {
                    Ok(done) => done,
                    Err(cause) => {
                        self.session_dirty = true;
                        return Err(cause);
                    }
                };
                if insert_id != 0 {
                    // Server-side LAST_INSERT_ID() is now non-zero for this session.
                    self.session_dirty = true;
                }
                self.last_insert_id = insert_id;
                Ok(rows)
            }
            Prepared::Emulated => {
                if values.is_empty() {
                    return self.text(&sql.source, options.mysql_stream.as_ref()).await;
                }
                sql.check_emulation()?;
                let mode = if sql.has_sensitive_quotes() {
                    let syntax = self.placeholder_syntax().await?;
                    sql.check_string_mode(syntax)?;
                    syntax.backslash_strings()
                } else {
                    self.backslash_escapes().await?
                };
                let query = sql.render_into(|i, output| match &values[i] {
                    BoundValue::String(bytes) => {
                        if options.national_strings {
                            output.push('N');
                        }
                        push_quote(output, bytes, mode);
                    }
                    value => push_literal(output, value, true),
                });
                self.text(&query, options.mysql_stream.as_ref()).await
            }
            _ => Err(DbError {
                state: "HY000".into(),
                code: None,
                detail: "Mismatched database driver".into(),
            }),
        }
    }

    pub async fn run_unprepared(&mut self, sql: &str) -> Result<Rows, DbError> {
        self.text(sql, None).await
    }

    async fn text(&mut self, sql: &str, owner: Option<&Arc<()>>) -> Result<Rows, DbError> {
        self.check_pending().await?;
        if !self.db_changed && contains_use_keyword(sql.as_bytes()) {
            self.db_changed = true;
        }
        if !self.session_dirty && !is_clean_dml_sql(sql.as_bytes()) {
            self.session_dirty = true;
        }
        if !self.options.multiple {
            require_single_statement(self.connection()?, sql).await?;
        }
        if let Some(owner) = owner {
            self.session_dirty = true;
            return self
                .stream(stream::Query::Text(sql.as_bytes().to_vec()), owner)
                .await?
                .rows
                .into_php();
        }
        let result = match self.connection()?.query_iter(sql).await {
            Ok(result) => result,
            Err(cause) => {
                self.session_dirty = true;
                return Err(error(cause));
            }
        };
        let (rows, insert_id) = match buffered(result).await {
            Ok(done) => done,
            Err(cause) => {
                self.session_dirty = true;
                return Err(cause);
            }
        };
        if insert_id != 0 {
            self.session_dirty = true;
        }
        self.last_insert_id = insert_id;
        Ok(rows)
    }

    pub async fn exec(&mut self, sql: &str) -> Result<Rows, DbError> {
        let rows = self.run_unprepared(sql).await?;
        // PHP exec() has no statement on which to consume a SELECT's result.
        self.pending_exec_rows = !rows.columns.is_empty();
        Ok(rows)
    }

    async fn check_pending(&mut self) -> Result<(), DbError> {
        if self.active.as_ref().is_some_and(stream::Active::abandoned) {
            self.finish_stream().await;
        }
        if !self.pending_exec_rows && self.active.is_none() {
            return Ok(());
        }
        Err(DbError {
            state: "HY000".into(),
            code: Some(2014),
            detail: "Cannot execute queries while other unbuffered queries are active.  \
             Consider using PDOStatement::fetchAll().  Alternatively, if your code \
             is only ever going to run against mysql, you may enable query buffering \
             by setting the Pdo\\Mysql::ATTR_USE_BUFFERED_QUERY attribute."
                .into(),
        })
    }

    async fn backslash_escapes(&mut self) -> Result<bool, DbError> {
        self.check_pending().await?;
        if let Some(enabled) = self.observed_backslashes() {
            return Ok(enabled);
        }
        // An error packet has no status flags, and the client discards the
        // previous OK packet. Ask for the mode before quoting again.
        Ok(!self
            .sql_mode()
            .await?
            .split(',')
            .any(|value| value == "NO_BACKSLASH_ESCAPES"))
    }

    async fn sql_mode(&mut self) -> Result<String, DbError> {
        let mode: Option<String> = self
            .connection()?
            .query_first("SELECT @@SESSION.sql_mode")
            .await
            .map_err(error)?;
        Ok(mode.unwrap_or_default())
    }

    pub async fn placeholder_syntax(&mut self) -> Result<Syntax, DbError> {
        self.check_pending().await?;
        let mode = self.sql_mode().await?;
        Ok(Syntax::Mysql {
            backslash_strings: !mode.split(',').any(|value| value == "NO_BACKSLASH_ESCAPES"),
            ansi_quotes: mode.split(',').any(|value| value == "ANSI_QUOTES"),
        })
    }

    pub async fn quote(&mut self, bytes: &[u8]) -> Result<String, DbError> {
        let backslashes = self.backslash_escapes().await?;
        Ok(quote(bytes, backslashes))
    }

    pub fn transaction_state(&self) -> Option<bool> {
        // Like mysqlnd, retain PDO's last observed status after an error packet.
        self.connection.as_ref()?.last_ok_packet().map(|packet| {
            packet
                .status_flags()
                .contains(mysql_async::consts::StatusFlags::SERVER_STATUS_IN_TRANS)
        })
    }

    pub fn last_id(&self) -> String {
        self.last_insert_id.to_string()
    }

    fn closed() -> DbError {
        DbError {
            state: "HY000".into(),
            code: Some(2006),
            detail: "MySQL server has gone away".into(),
        }
    }

    fn connection(&mut self) -> Result<&mut Conn, DbError> {
        self.connection.as_mut().ok_or_else(Self::closed)
    }
}

async fn buffered<P: Protocol>(mut result: QueryResult<'_, '_, P>) -> Result<(Rows, u64), DbError> {
    if result.columns_ref().is_empty() {
        let affected = result.affected_rows();
        let insert_id = result.last_insert_id().unwrap_or(0);
        result.drop_result().await.map_err(error)?;
        return Ok((
            Rows {
                affected,
                ..Rows::default()
            },
            insert_id,
        ));
    }
    let columns = result.columns_ref().to_vec();
    let mut values = RowBuffer::with_capacity(columns.len(), 4);
    while let Some(row) = result.next().await.map_err(error)? {
        values::push_pdo_row(&mut values, row, &columns);
    }
    let affected = result.affected_rows();
    let insert_id = result.last_insert_id().unwrap_or(0);
    result.drop_result().await.map_err(error)?;
    Ok((values::finish(columns, values, affected), insert_id))
}

/// Strictly conservative allowlist for single-statement DML (`SELECT`, `INSERT`,
/// `UPDATE`, `DELETE`, `REPLACE`) that cannot mutate MySQL session state.
/// Any multi-statement, comment, user variable (`@`), locking/session function,
/// or non-DML statement returns `false`, forcing full `COM_RESET_CONNECTION`.
pub(super) fn is_clean_dml_sql(sql: &[u8]) -> bool {
    let mut rest = sql;
    while let Some(&b) = rest.first() {
        if b.is_ascii_whitespace() {
            rest = &rest[1..];
        } else {
            break;
        }
    }
    let first_len = rest.iter().take_while(|b| b.is_ascii_alphabetic()).count();
    let first = &rest[..first_len];
    let after_first = rest.get(first_len).copied();
    let starts_dml = after_first.is_some_and(|b| b.is_ascii_whitespace())
        && (first.eq_ignore_ascii_case(b"SELECT")
            || first.eq_ignore_ascii_case(b"INSERT")
            || first.eq_ignore_ascii_case(b"UPDATE")
            || first.eq_ignore_ascii_case(b"DELETE")
            || first.eq_ignore_ascii_case(b"REPLACE"));
    if !starts_dml {
        return false;
    }
    // Reject multi-statements, user/system variables, and SQL comments.
    for (i, &b) in rest.iter().enumerate() {
        match b {
            b';' | b'@' | b'#' => return false,
            b'/' if rest.get(i + 1) == Some(&b'*') => return false,
            b'-' if rest.get(i + 1) == Some(&b'-') => return false,
            _ => {}
        }
    }
    // Scan identifier tokens for session-mutating clauses or functions.
    let mut i = first_len;
    while i < rest.len() {
        if is_ident_byte(rest[i]) {
            let start = i;
            while i < rest.len() && is_ident_byte(rest[i]) {
                i += 1;
            }
            let word = &rest[start..i];
            if word.eq_ignore_ascii_case(b"OUTFILE")
                || word.eq_ignore_ascii_case(b"DUMPFILE")
                || word.eq_ignore_ascii_case(b"GET_LOCK")
                || word.eq_ignore_ascii_case(b"RELEASE_LOCK")
                || word.eq_ignore_ascii_case(b"RELEASE_ALL_LOCKS")
                || word.eq_ignore_ascii_case(b"SQL_CALC_FOUND_ROWS")
                || word.eq_ignore_ascii_case(b"FOUND_ROWS")
                || word.eq_ignore_ascii_case(b"ROW_COUNT")
                || word.eq_ignore_ascii_case(b"LAST_INSERT_ID")
                || word.eq_ignore_ascii_case(b"TEMPORARY")
                || word.eq_ignore_ascii_case(b"PREPARE")
                || word.eq_ignore_ascii_case(b"DEALLOCATE")
                || word.eq_ignore_ascii_case(b"LOCK")
                || word.eq_ignore_ascii_case(b"UNLOCK")
            {
                return false;
            }
        } else {
            i += 1;
        }
    }
    true
}

pub(super) fn build_reset_sql(
    database: &str,
    charset: Option<&str>,
    settings: &MysqlOptions,
) -> (String, String) {
    let mut keep_db = String::new();
    let push_stmt = |buf: &mut String, stmt: &str| {
        if !buf.is_empty() {
            buf.push_str("; ");
        }
        buf.push_str(stmt);
    };
    if let Some(charset) = charset {
        push_stmt(&mut keep_db, &format!("SET NAMES {charset}"));
    }
    if settings.ignore_space {
        push_stmt(
            &mut keep_db,
            "SET sql_mode = CONCAT_WS(',', NULLIF(@@sql_mode, ''), 'IGNORE_SPACE')",
        );
    }
    if !settings.autocommit {
        push_stmt(&mut keep_db, "SET autocommit=0");
    }
    let with_db = if database.is_empty() {
        keep_db.clone()
    } else {
        let escaped = database.replace('`', "``");
        let mut buf = format!("USE `{escaped}`");
        if !keep_db.is_empty() {
            buf.push_str("; ");
            buf.push_str(&keep_db);
        }
        buf
    };
    (with_db, keep_db)
}

/// Conservative check for whether SQL text might contain a top-level `USE`
/// statement that changes the connection's selected database.
pub(super) fn contains_use_keyword(sql: &[u8]) -> bool {
    sql.windows(3).enumerate().any(|(i, window)| {
        window.eq_ignore_ascii_case(b"use")
            && (i == 0 || !is_ident_byte(sql[i - 1]))
            && sql.get(i + 3).is_none_or(|&b| !is_ident_byte(b))
    })
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

async fn configure(connection: &mut Conn, settings: &MysqlOptions) -> Result<(), DbError> {
    if settings.ignore_space {
        connection
            .query_drop("SET sql_mode = CONCAT_WS(',', NULLIF(@@sql_mode, ''), 'IGNORE_SPACE')")
            .await
            .map_err(error)?;
    }
    if let Some(init) = &settings.init {
        if !settings.multiple {
            require_single_statement(connection, init).await?;
        }
        connection.query_drop(init).await.map_err(error)?;
    }
    if !settings.autocommit {
        connection
            .query_drop("SET autocommit=0")
            .await
            .map_err(error)?;
    }
    Ok(())
}

/// The client library always advertises multi-statements. Only SQL containing a
/// statement separator needs a server-side single-statement parse check before
/// running as a text query.
async fn require_single_statement(connection: &mut Conn, sql: &str) -> Result<(), DbError> {
    if !sql.as_bytes().contains(&b';') {
        return Ok(());
    }
    let statement = connection.prep(sql).await.map_err(error)?;
    connection.close(statement).await.map_err(error)
}

pub fn error(error: mysql_async::Error) -> DbError {
    match error {
        mysql_async::Error::Driver(mysql_async::DriverError::LocalInfile(_)) => DbError {
            state: "HY000".into(),
            code: Some(2068),
            detail: "MySQL LOCAL INFILE is disabled because it lets the server read client files"
                .into(),
        },
        mysql_async::Error::Server(error) => DbError {
            state: error.state,
            code: Some(i64::from(error.code)),
            detail: error.message,
        },
        error => DbError {
            state: "HY000".into(),
            code: Some(2002),
            detail: error.to_string(),
        },
    }
}

#[cfg(test)]
mod clean_sql_tests {
    use super::is_clean_dml_sql;

    #[test]
    fn allows_plain_single_statement_dml() {
        assert!(is_clean_dml_sql(
            b"SELECT id, randomNumber FROM World WHERE id = ?"
        ));
        assert!(is_clean_dml_sql(b"  select id, message from Fortune"));
        assert!(is_clean_dml_sql(
            b"UPDATE World SET randomNumber = ? WHERE id = ?"
        ));
        assert!(is_clean_dml_sql(
            b"DELETE FROM sessions WHERE expires_at < ?"
        ));
        assert!(is_clean_dml_sql(b"INSERT INTO logs (msg) VALUES (?)"));
        assert!(is_clean_dml_sql(b"REPLACE INTO cache (k, v) VALUES (?, ?)"));
    }

    #[test]
    fn rejects_session_mutating_and_ambiguous_sql() {
        assert!(!is_clean_dml_sql(b"SET @x = 1"));
        assert!(!is_clean_dml_sql(b"SET NAMES utf8mb4"));
        assert!(!is_clean_dml_sql(b"USE other_db"));
        assert!(!is_clean_dml_sql(b"BEGIN"));
        assert!(!is_clean_dml_sql(b"START TRANSACTION"));
        assert!(!is_clean_dml_sql(b"CREATE TEMPORARY TABLE t (id INT)"));
        assert!(!is_clean_dml_sql(b"SELECT 1; SET @x = 2"));
        assert!(!is_clean_dml_sql(b"SELECT @x := 1"));
        assert!(!is_clean_dml_sql(b"SELECT @@session.sql_mode"));
        assert!(!is_clean_dml_sql(b"SELECT GET_LOCK('k', 1)"));
        assert!(!is_clean_dml_sql(b"SELECT SQL_CALC_FOUND_ROWS id FROM t"));
        assert!(!is_clean_dml_sql(b"SELECT LAST_INSERT_ID()"));
        assert!(!is_clean_dml_sql(b"SELECT * FROM t INTO OUTFILE '/tmp/x'"));
        assert!(!is_clean_dml_sql(b"SELECT 1 /* comment */"));
        assert!(!is_clean_dml_sql(b"SELECT 1 -- comment"));
        assert!(!is_clean_dml_sql(b"SELECT 1 # comment"));
        assert!(!is_clean_dml_sql(b"SELECT 1 FROM t LOCK IN SHARE MODE"));
    }
}
