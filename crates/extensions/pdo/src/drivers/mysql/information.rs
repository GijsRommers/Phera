//! `PDO::ATTR_SERVER_INFO` reports the server's current MySQL counters.

use std::collections::HashMap;

use mysql_async::prelude::Queryable;

use super::{Mysql, error};
use crate::errors::DbError;

impl Mysql {
    pub fn connection_status(&self) -> String {
        if self.unix_socket {
            "Localhost via UNIX socket".into()
        } else {
            format!("{} via TCP/IP", self.host)
        }
    }

    pub async fn server_info(&mut self) -> Result<String, DbError> {
        self.check_pending()?;
        let rows: Vec<(String, u64)> = self
            .connection
            .query(
                "SHOW GLOBAL STATUS WHERE Variable_name IN ('Uptime', 'Threads_connected', \
            'Questions', 'Slow_queries', 'Opened_tables', 'Flush_commands', 'Open_tables')",
            )
            .await
            .map_err(error)?;
        let stats: HashMap<_, _> = rows.into_iter().collect();
        let get = |key| stats.get(key).copied().unwrap_or(0);
        let uptime = get("Uptime");
        let questions = get("Questions");
        let qps = questions as f64 / uptime.max(1) as f64;
        Ok(format!(
            "Uptime: {uptime}  Threads: {}  Questions: {questions}  Slow queries: {}  \
            Opens: {}  Flush tables: {}  Open tables: {}  Queries per second avg: {qps:.3}",
            get("Threads_connected"),
            get("Slow_queries"),
            get("Opened_tables"),
            get("Flush_commands"),
            get("Open_tables")
        ))
    }
}
