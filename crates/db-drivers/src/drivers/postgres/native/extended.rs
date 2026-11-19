//! `pg_query_params($db, 'SELECT $1::integer', ['42'])` binds PostgreSQL text parameters.

use tokio_postgres::Statement;
use tokio_postgres::types::{ToSql, Type};

use super::{NativeColumn, NativeResult, NativeSequence, Postgres, error};
use crate::drivers::postgres::observations::Header;
use crate::drivers::postgres::protocol::TextParameter;
use crate::errors::DbError;
use crate::parameters::BoundValue;

#[derive(Clone)]
pub(super) struct NativeStatement {
    statement: Statement,
    columns: Vec<NativeColumn>,
}

impl Postgres {
    pub async fn native_prepare(
        &mut self,
        name: &str,
        sql: &str,
    ) -> Result<NativeSequence, DbError> {
        if !name.is_empty() && self.native()?.statements.contains_key(name) {
            return Err(DbError {
                state: "42P05".into(),
                code: None,
                detail: format!("ERROR:  prepared statement \"{name}\" already exists"),
            });
        }
        let statement = self.prepare_native_statement(sql).await?;
        self.native_mut()?
            .statements
            .insert(name.to_string(), statement);
        Ok(vec![NativeResult {
            command: "PREPARE".into(),
            ..NativeResult::default()
        }])
    }

    pub async fn native_execute(
        &mut self,
        name: &str,
        params: &[BoundValue],
    ) -> Result<NativeSequence, DbError> {
        let Some(statement) = self.native()?.statements.get(name).cloned() else {
            return Err(DbError {
                state: "26000".into(),
                code: None,
                detail: format!("ERROR:  prepared statement \"{name}\" does not exist"),
            });
        };
        self.native_bound(&statement, params).await
    }

    pub async fn native_query_params(
        &mut self,
        sql: &str,
        params: &[BoundValue],
    ) -> Result<NativeSequence, DbError> {
        let statement = self.prepare_native_statement(sql).await?;
        self.native_bound(&statement, params).await
    }

    async fn prepare_native_statement(&mut self, sql: &str) -> Result<NativeStatement, DbError> {
        let observations = self.native()?.observations.clone();
        observations.capture();
        let prepared = self.prepare_query(sql).await;
        let description = observations
            .finish()
            .into_iter()
            .find_map(|header| match header {
                Header::Columns(columns) => Some(columns),
                Header::Command(_) => None,
            });
        let statement = prepared?;
        let columns = description.unwrap_or_else(|| {
            statement
                .columns()
                .iter()
                .map(|column| {
                    let kind = column.type_();
                    NativeColumn {
                        name: column.name().into(),
                        table: column.table_oid().unwrap_or(0),
                        oid: kind.oid(),
                        size: -1,
                        kind: kind.name().into(),
                    }
                })
                .collect()
        });
        Ok(NativeStatement { statement, columns })
    }

    async fn native_bound(
        &mut self,
        prepared: &NativeStatement,
        params: &[BoundValue],
    ) -> Result<NativeSequence, DbError> {
        let params: Vec<TextParameter> = params.iter().map(TextParameter::from).collect();
        let refs: Vec<&(dyn ToSql + Sync)> = params
            .iter()
            .map(|value| value as &(dyn ToSql + Sync))
            .collect();
        let statement = &prepared.statement;
        let observations = self.native()?.observations.clone();
        observations.capture();
        let mut result = NativeResult {
            columns: prepared.columns.clone(),
            ..NativeResult::default()
        };
        let response = if result.columns.is_empty() {
            self.client
                .execute(statement, &refs)
                .await
                .map(|affected| (Vec::new(), affected))
        } else {
            self.client.query(statement, &refs).await.map(|rows| {
                let count = rows.len() as u64;
                (rows, count)
            })
        };
        if let Some(Header::Command(command)) = observations.finish().pop_back() {
            result.command = command;
        }
        let (rows, affected) = response.map_err(|cause| {
            let context = cause
                .as_db_error()
                .and_then(|e| e.where_())
                .map(str::to_owned);
            let mut error = error(cause, "");
            if let Some(context) = context {
                error.detail.push_str("\nCONTEXT:  ");
                error.detail.push_str(&context);
            }
            error
        })?;
        result.affected = affected;
        if !rows.is_empty() {
            // The existing driver receives binary values. Ask PostgreSQL to
            // render those values in its session's text format without
            // re-executing the caller's statement.
            let sql = format!(
                "SELECT {}",
                statement
                    .columns()
                    .iter()
                    .enumerate()
                    .map(|(index, col)| {
                        let expression = format!("${}", index + 1);
                        match *col.type_() {
                            Type::BOOL => format!("pg_catalog.boolout({expression})::text"),
                            Type::BPCHAR => format!("pg_catalog.bpcharout({expression})::text"),
                            Type::INET => format!("pg_catalog.inet_out({expression})::text"),
                            _ => format!("{expression}::text"),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            );
            for row in rows {
                result
                    .values
                    .push(super::super::text::format_row(&self.client, &row, &sql).await?);
            }
        }
        Ok(vec![result])
    }
}
