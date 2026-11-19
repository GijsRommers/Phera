//! `Pdo\Pgsql::ATTR_DISABLE_PREPARES` uses a native bind without a named statement.

use std::future::poll_fn;
use std::pin::pin;

use futures_core::Stream;
use tokio_postgres::types::{ToSql, Type};

use super::{Postgres, Row, TextParameter, error, text};
use crate::drivers::RawRows;
use crate::errors::DbError;
use crate::parameters::BoundValue;
use crate::placeholders::Sql;

impl Postgres {
    pub(super) async fn execute_unnamed(
        &mut self,
        sql: &Sql,
        values: &[BoundValue],
    ) -> Result<RawRows, DbError> {
        let query = sql.render(|index| format!("${}", index + 1));
        let parameters: Vec<_> = values.iter().map(TextParameter::from).collect();
        let parameters = parameters
            .iter()
            .map(|value| (value as &(dyn ToSql + Sync), Type::UNKNOWN));
        let stream = self
            .client
            .query_typed_raw(&query, parameters)
            .await
            .map_err(|failure| error(failure, &query))?;
        let mut stream = pin!(stream);
        let mut columns = Vec::new();
        let mut binary = Vec::new();
        while let Some(row) = poll_fn(|context| stream.as_mut().poll_next(context)).await {
            let row = row.map_err(|failure| error(failure, &query))?;
            if columns.is_empty() {
                self.retire |= row
                    .columns()
                    .iter()
                    .any(|column| Type::from_oid(column.type_().oid()).is_none());
                columns = row
                    .columns()
                    .iter()
                    .map(crate::metadata::postgres)
                    .collect();
            }
            binary.push(row);
        }
        let affected = stream.rows_affected().unwrap_or_default();
        let mut rows = Vec::with_capacity(binary.len());
        for row in binary {
            let text = text::convert(&self.client, &row, &query).await?;
            rows.push(Row { binary: row, text });
        }
        Ok(RawRows::Postgres {
            columns,
            rows,
            affected,
        })
    }
}
