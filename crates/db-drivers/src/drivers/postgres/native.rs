//! `pg_query($connection, $sql)` returns text cells with PostgreSQL column metadata.

mod copy;
mod extended;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use tokio_postgres::types::Type;
use tokio_postgres::{Notification, SimpleQueryMessage, Statement};

use super::{
    Postgres, error,
    observations::{Header, Observations},
};
use crate::drivers::connection_error;
use crate::errors::DbError;

pub struct NativeColumn {
    pub name: String,
    pub oid: u32,
    pub table: u32,
    pub size: i16,
    pub kind: String,
}

impl NativeColumn {
    pub fn kind(oid: u32) -> String {
        Type::from_oid(oid).map_or_else(|| oid.to_string(), |kind| kind.name().into())
    }

    fn unknown(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            oid: 0,
            table: 0,
            size: -1,
            kind: String::new(),
        }
    }
}

#[derive(Default)]
pub struct NativeResult {
    pub columns: Vec<NativeColumn>,
    pub values: Vec<Vec<Option<Vec<u8>>>>,
    pub affected: u64,
    pub command: String,
    pub error: Option<DbError>,
}

impl NativeResult {
    pub fn status(&self) -> i64 {
        if self.error.is_some() {
            7
        } else if !self.columns.is_empty() {
            2
        } else {
            i64::from(!self.command.is_empty())
        }
    }
}

pub type NativeSequence = Vec<NativeResult>;

pub(super) struct State {
    observations: Arc<Observations>,
    statements: HashMap<String, Statement>,
}

impl State {
    pub fn new(observations: Arc<Observations>) -> Self {
        Self {
            observations,
            statements: HashMap::new(),
        }
    }

    pub fn reset(&mut self) {
        self.statements.clear();
        self.observations.reset();
    }
}

impl Postgres {
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
        self.native
            .as_ref()
            .map_or(0, |state| state.observations.pid())
    }

    pub fn notices(&self, clear: bool) -> Vec<String> {
        self.native
            .as_ref()
            .map_or_else(Vec::new, |state| state.observations.notices(clear))
    }

    pub fn notification(&self) -> Option<Notification> {
        self.native
            .as_ref()
            .and_then(|state| state.observations.notification())
    }

    pub fn transaction_status(&self) -> i64 {
        if self.is_broken() {
            return 4;
        }
        match self.transaction.load(Ordering::Acquire) {
            b'T' => 2,
            b'E' => 3,
            _ => 0,
        }
    }

    pub async fn native_query(&mut self, sql: &str) -> Result<NativeSequence, DbError> {
        let observations = self.native()?.observations.clone();
        observations.capture();
        let response = self.client.simple_query(sql).await;
        let mut headers = observations.finish();
        let mut finished = Vec::new();
        let mut result = NativeResult::default();
        let messages = match response {
            Ok(messages) => messages,
            Err(cause) => {
                return Ok(vec![NativeResult {
                    error: Some(error(cause, sql)),
                    ..NativeResult::default()
                }]);
            }
        };
        for message in messages {
            match message {
                SimpleQueryMessage::RowDescription(columns) => {
                    result.columns = match headers.pop_front() {
                        Some(Header::Columns(columns)) => columns,
                        _ => columns
                            .iter()
                            .map(|column| NativeColumn::unknown(column.name()))
                            .collect(),
                    };
                }
                SimpleQueryMessage::Row(row) => {
                    let mut values = Vec::with_capacity(row.len());
                    for index in 0..row.len() {
                        let text = row.try_get(index).map_err(|e| error(e, sql))?;
                        values.push(text.map(|text| text.as_bytes().to_vec()));
                    }
                    result.values.push(values);
                }
                SimpleQueryMessage::CommandComplete(count) => {
                    result.affected = count;
                    result.command = match headers.pop_front() {
                        Some(Header::Command(tag)) => tag,
                        _ => String::new(),
                    };
                    finished.push(std::mem::take(&mut result));
                }
                _ => {}
            }
        }
        if finished.is_empty() {
            finished.push(result);
        }
        Ok(finished)
    }

    pub async fn parameter_status(&self, name: &str) -> Result<Option<String>, DbError> {
        let row = self
            .client
            .query_one("SELECT pg_catalog.current_setting($1, true)", &[&name])
            .await
            .map_err(|cause| error(cause, ""))?;
        row.try_get(0).map_err(|cause| error(cause, ""))
    }
}
