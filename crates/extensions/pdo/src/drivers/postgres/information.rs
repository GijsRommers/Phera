//! `PDO::ATTR_SERVER_INFO` reports the PostgreSQL backend and session settings.

use super::{Postgres, error};
use crate::errors::DbError;

impl Postgres {
    pub fn connection_status(&self) -> &'static str {
        if self.client.is_closed() {
            "Bad connection."
        } else {
            "Connection OK; waiting to send."
        }
    }

    pub async fn server_info(&self) -> Result<String, DbError> {
        let sql = "SELECT pg_backend_pid(), current_setting('client_encoding'), \
            current_setting('is_superuser'), current_setting('session_authorization'), \
            current_setting('DateStyle')";
        let row = self
            .client
            .query_one(sql, &[])
            .await
            .map_err(|failure| error(failure, sql))?;
        let pid: i32 = row.get(0);
        let encoding: &str = row.get(1);
        let superuser: &str = row.get(2);
        let user: &str = row.get(3);
        let date_style: &str = row.get(4);
        Ok(format!(
            "PID: {pid}; Client Encoding: {encoding}; Is Superuser: {superuser}; \
            Session Authorization: {user}; Date Style: {date_style}"
        ))
    }
}
