//! `pg_query($connection, $sql)` returns text cells with PostgreSQL column metadata.

mod copy;
mod extended;
mod lob_file;

use std::collections::HashSet;

use postgres_protocol::message::backend::Message;

use super::wire::rows::Cells;
use super::{Postgres, affected, values, wire};
use crate::drivers::connection_error;
use crate::errors::DbError;
use crate::parameters::BoundValue;

#[derive(Clone)]
pub struct NativeColumn {
    pub name: String,
    pub oid: u32,
    pub table: u32,
    pub size: i16,
    pub kind: String,
}

impl NativeColumn {
    pub fn kind(oid: u32) -> String {
        values::type_name(oid)
    }
}

#[derive(Default)]
pub struct NativeResult {
    pub columns: Vec<NativeColumn>,
    pub values: Vec<Vec<Option<Vec<u8>>>>,
    pub affected: u64,
    pub command: String,
    pub error: Option<DbError>,
    pub copy_in: bool,
}

impl NativeResult {
    pub fn status(&self) -> i64 {
        if self.error.is_some() {
            7
        } else if self.copy_in {
            4
        } else if !self.columns.is_empty() {
            2
        } else {
            i64::from(!self.command.is_empty())
        }
    }
}

pub type NativeSequence = Vec<NativeResult>;

#[derive(Default)]
pub(super) struct State {
    statements: HashSet<String>,
    copy: bool,
}

impl State {
    pub fn reset(&mut self) {
        self.statements.clear();
        self.copy = false;
    }

    pub fn copying(&self) -> bool {
        self.copy
    }
}

impl Postgres {
    pub fn can_cancel_native_query(sql: &str) -> bool {
        !copy::is_copy_from_stdin(sql)
    }

    fn native(&self) -> Result<&State, DbError> {
        self.native
            .as_ref()
            .ok_or_else(|| connection_error("Missing pgsql state"))
    }

    fn native_mut(&mut self) -> Result<&mut State, DbError> {
        self.native
            .as_mut()
            .ok_or_else(|| connection_error("Missing pgsql state"))
    }

    pub fn pid(&self) -> i32 {
        self.client.pid()
    }

    pub fn notices(&mut self, clear: bool) -> Vec<String> {
        self.client.notices(clear)
    }

    pub async fn notification(&mut self) -> Result<Option<wire::Notification>, DbError> {
        self.consume_input().await?;
        Ok(self.client.notification())
    }

    pub async fn consume_input(&mut self) -> Result<(), DbError> {
        if self.lazy.is_none() && !self.native()?.copy {
            self.client.consume_input().await?;
        }
        Ok(())
    }

    pub fn transaction_status(&self) -> i64 {
        if self.is_broken() {
            return 4;
        }
        match self.client.status() {
            b'T' => 2,
            b'E' => 3,
            _ => 0,
        }
    }

    pub async fn native_query(&mut self, sql: &str) -> Result<NativeSequence, DbError> {
        if self.native()?.copy {
            return Err(DbError::general("COPY is in progress"));
        }
        if !Self::can_cancel_native_query(sql) {
            self.begin_copy(sql).await?;
            return Ok(vec![NativeResult {
                copy_in: true,
                ..NativeResult::default()
            }]);
        }
        self.client.simple(sql).await?;
        self.collect_native(sql, false).await
    }

    async fn collect_native(
        &mut self,
        sql: &str,
        context: bool,
    ) -> Result<NativeSequence, DbError> {
        let mut completed = Vec::new();
        let mut result = NativeResult::default();
        loop {
            match self.client.next().await? {
                Message::RowDescription(body) => {
                    result.columns = wire::rows::fields(&body)
                        .map_err(|error| self.client.invalid(error))?
                        .into_iter()
                        .map(|field| NativeColumn {
                            name: field.name,
                            oid: field.oid,
                            table: field.table,
                            size: field.size,
                            kind: NativeColumn::kind(field.oid),
                        })
                        .collect();
                }
                Message::DataRow(body) => {
                    let mut cells = Cells::new(&body);
                    if cells.len() != result.columns.len() {
                        return Err(self
                            .client
                            .invalid("PostgreSQL row width differs from its description"));
                    }
                    let row = (0..cells.len())
                        .map(|_| cells.next().map(|value| value.map(<[u8]>::to_vec)))
                        .collect::<Result<_, _>>()
                        .map_err(|error| self.client.invalid(error))?;
                    cells.finish().map_err(|error| self.client.invalid(error))?;
                    result.values.push(row);
                }
                Message::CommandComplete(command) => {
                    let tag = command.tag().map_err(|error| self.client.invalid(error))?;
                    result.affected = affected(tag);
                    result.command = tag.into();
                    completed.push(std::mem::take(&mut result));
                }
                Message::EmptyQueryResponse => completed.push(NativeResult::default()),
                Message::ReadyForQuery(_) => {
                    if completed.is_empty() {
                        completed.push(result);
                    }
                    return Ok(completed);
                }
                Message::ErrorResponse(error) => {
                    let error = self.client.failure(error, sql, context).await;
                    completed.push(NativeResult {
                        error: Some(error),
                        ..NativeResult::default()
                    });
                    return Ok(completed);
                }
                Message::ParseComplete
                | Message::BindComplete
                | Message::CloseComplete
                | Message::NoData => {}
                _ => return Err(self.client.invalid("unexpected PostgreSQL result response")),
            }
        }
    }

    /// A one-shot statement used by internal catalog and large-object calls.
    pub(super) async fn single_row(
        &mut self,
        sql: &str,
        params: &[BoundValue],
    ) -> Result<Vec<Option<Vec<u8>>>, DbError> {
        self.client.extended("", Some(sql), params, &[]).await?;
        let mut result = self
            .collect_native(sql, false)
            .await?
            .pop()
            .unwrap_or_default();
        if let Some(error) = result.error {
            return Err(error);
        }
        if result.values.len() != 1 {
            return Err(connection_error(
                "PostgreSQL did not return exactly one row",
            ));
        }
        Ok(result.values.pop().unwrap_or_default())
    }

    pub async fn parameter_status(&mut self, name: &str) -> Result<Option<String>, DbError> {
        if let Some(value) = self.client.parameter(name) {
            return Ok(Some(value.to_owned()));
        }
        let row = self
            .single_row(
                "SELECT pg_catalog.current_setting($1, true)",
                &[BoundValue::String(name.as_bytes().to_vec())],
            )
            .await?;
        Ok(row
            .into_iter()
            .next()
            .flatten()
            .map(|value| String::from_utf8_lossy(&value).into_owned()))
    }
}
