//! MySQL program status and result sets used by `$db->query($sql)`.

use std::sync::Arc;

use mysql_async::consts::StatusFlags;
use mysql_async::prelude::Queryable;
use mysql_async::{ChangeUserOpts, Params, Row, Statement};

use super::{Mysql, error};
use crate::drivers::RawRows;
use crate::errors::DbError;

#[derive(Default)]
pub struct Reply {
    pub affected: u64,
    pub insert_id: u64,
    pub info: String,
    pub warnings: u16,
}

pub struct Outcome {
    pub rows: RawRows,
    pub reply: Reply,
}

impl Mysql {
    pub fn reusable(&self) -> bool {
        self.reusable
    }

    pub fn thread_id(&self) -> u32 {
        self.connection
            .as_ref()
            .map_or(0, |connection| connection.id())
    }

    pub fn default_charset(&self) -> &str {
        self.charset.as_deref().unwrap_or("utf8mb4")
    }

    pub async fn select_db(&mut self, name: &str) -> Result<(), DbError> {
        self.check_pending().await?;
        let name = identifier(name);
        self.connection()?
            .query_drop(format!("USE {name}"))
            .await
            .map_err(error)
    }

    pub async fn set_charset(&mut self, name: &str) -> Result<(), DbError> {
        self.check_pending().await?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(DbError {
                state: "HY000".into(),
                code: Some(2019),
                detail: format!("Invalid character set was provided ({name})"),
            });
        }
        self.connection()?
            .query_drop(format!("SET NAMES {name}"))
            .await
            .map_err(error)
    }

    pub async fn change_user(
        &mut self,
        user: String,
        password: String,
        database: Option<String>,
    ) -> Result<(), DbError> {
        self.check_pending().await?;
        // The pool key identifies the original user. COM_CHANGE_USER changes
        // the credentials the client would use even on its next reset.
        self.reusable = false;
        let options = ChangeUserOpts::new()
            .with_user(Some(user))
            .with_pass(Some(password))
            .with_db_name(database);
        self.connection()?.change_user(options).await.map_err(error)
    }

    pub async fn prepare_native(&mut self, sql: &[u8]) -> Result<Statement, DbError> {
        self.check_pending().await?;
        self.connection()?.prep(sql).await.map_err(error)
    }

    pub async fn execute_native(
        &mut self,
        statement: &Statement,
        params: Params,
    ) -> Result<Outcome, DbError> {
        self.check_pending().await?;
        let mut result = self
            .connection()?
            .exec_iter(statement, params)
            .await
            .map_err(error)?;
        let columns = result.columns_ref().to_vec();
        let reply = Reply {
            affected: result.affected_rows(),
            insert_id: result.last_insert_id().unwrap_or(0),
            info: result.info().into_owned(),
            warnings: result.warnings(),
        };
        let rows: Vec<Row> = result.collect().await.map_err(error)?;
        result.drop_result().await.map_err(error)?;
        Ok(Outcome {
            rows: RawRows::Mysql {
                columns,
                rows,
                affected: reply.affected,
            },
            reply,
        })
    }

    /// The text protocol reader waits for an explicit fetch request after its header.
    pub async fn start_native_stream(
        &mut self,
        sql: Vec<u8>,
        owner: &Arc<()>,
    ) -> Result<Outcome, DbError> {
        self.check_pending().await?;
        self.validate_native_text(&sql).await?;
        self.stream(super::stream::Query::Text(sql), owner).await
    }

    async fn validate_native_text(&mut self, sql: &[u8]) -> Result<(), DbError> {
        if sql.contains(&b';') {
            // mysql_async negotiates multi-statements and cannot switch them off.
            // Ask the server to parse SQL with separators before sending it as text.
            let connection = self.connection()?;
            let statement = connection.prep(sql).await.map_err(error)?;
            connection.close(statement).await.map_err(error)?;
        }
        Ok(())
    }

    pub async fn query_native(
        &mut self,
        sql: &[u8],
        multiple: bool,
    ) -> Result<Vec<Result<Outcome, DbError>>, DbError> {
        self.check_pending().await?;
        if !multiple {
            self.validate_native_text(sql).await?;
        }
        let mut result = self.connection()?.query_iter(sql).await.map_err(error)?;
        let mut outcomes = Vec::new();
        loop {
            let columns = result.columns_ref().to_vec();
            let mut reply = Reply {
                affected: result.affected_rows(),
                insert_id: result.last_insert_id().unwrap_or(0),
                info: result.info().into_owned(),
                warnings: result.warnings(),
            };
            let mut rows = Vec::new();
            // next() consumes the boundary and can surface a later statement's
            // server error at that point. Preserve the completed result ahead of it.
            let next_error = loop {
                match result.next().await {
                    Ok(Some(row)) => rows.push(row),
                    Ok(None) => break None,
                    Err(error_value) => break Some(error(error_value)),
                }
            };
            if !columns.is_empty() {
                reply.affected = rows.len() as u64;
            }
            outcomes.push(Ok(Outcome {
                rows: RawRows::Mysql {
                    columns,
                    rows,
                    affected: reply.affected,
                },
                reply,
            }));
            if let Some(error_value) = next_error {
                if multiple {
                    outcomes.push(Err(error_value));
                    // mysql_async may also surface the queued result error during cleanup.
                    let _ = result.drop_result().await;
                    return Ok(outcomes);
                }
                return Err(error_value);
            }
            if result.is_empty() {
                break;
            }
        }
        result.drop_result().await.map_err(error)?;
        Ok(outcomes)
    }

    pub fn backslashes_enabled(&self) -> bool {
        self.connection
            .as_ref()
            .and_then(|connection| connection.last_ok_packet())
            .is_none_or(|packet| {
                !packet
                    .status_flags()
                    .contains(StatusFlags::SERVER_STATUS_NO_BACKSLASH_ESCAPES)
            })
    }
}

pub fn identifier(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}
