//! Preserve PostgreSQL's real Bind-count error and aborted-transaction behavior.
//! tokio-postgres checks named statement arity locally. Its typed unnamed path
//! sends Parse and Bind together, allowing the server to diagnose missing values.

use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, Statement};

use super::protocol::{TextParameter, error};
use crate::errors::DbError;
use crate::parameters::BoundValue;
use crate::placeholders::Sql;

pub async fn under_bound(
    client: &Client,
    sql: &Sql,
    statement: &Statement,
    values: &[BoundValue],
    number: u64,
) -> DbError {
    let query = sql.render(|index| format!("${}", index + 1));
    let parameters: Vec<TextParameter> = values.iter().map(TextParameter::from).collect();
    let typed: Vec<_> = parameters
        .iter()
        .zip(statement.params())
        .map(|(value, kind)| (value as &(dyn ToSql + Sync), kind.clone()))
        .collect();
    match client.query_typed(&query, &typed).await {
        Err(failure) => {
            let mut failure = error(failure, &query);
            if failure.state == "08P01" {
                // PDO names native statements per PDO object, even if the
                // transport used an unnamed statement to submit this Bind.
                failure.detail = failure.detail.replace(
                    "prepared statement \"\"",
                    &format!("prepared statement \"pdo_stmt_{number:08x}\""),
                );
            }
            failure
        }
        Ok(_) => {
            crate::drivers::connection_error("PostgreSQL accepted an incomplete parameter list")
        }
    }
}
