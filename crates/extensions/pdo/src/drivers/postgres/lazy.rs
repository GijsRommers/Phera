//! `PDO::ATTR_PREFETCH => false` returns PostgreSQL rows one result at a time.

use std::collections::VecDeque;
use std::future::poll_fn;
use std::pin::Pin;
use std::sync::{Arc, Weak};

use futures_core::Stream;
use tokio_postgres::RowStream;
use tokio_postgres::types::Type;

use super::{Postgres, Row, error, text};
use crate::drivers::{Column, RawRows};
use crate::errors::DbError;

enum Pending {
    Stream {
        first: Option<tokio_postgres::Row>,
        source: Pin<Box<RowStream>>,
        query: String,
    },
    Formatted(VecDeque<Row>),
}

pub(super) struct Active {
    owner: Weak<()>,
    rows: Pending,
}

impl Postgres {
    pub fn discard_lazy(&mut self) {
        self.lazy = None;
    }

    pub fn close_lazy(&mut self, owner: &Arc<()>) {
        if self.lazy.as_ref().is_some_and(|lazy| lazy.owns(owner)) {
            self.discard_lazy();
        }
    }

    pub(super) async fn start_lazy(
        &mut self,
        stream: RowStream,
        mut columns: Vec<Column>,
        query: String,
        owner: &Arc<()>,
    ) -> Result<RawRows, DbError> {
        let mut source = Box::pin(stream);
        let Some(first) = next(&mut source, &query).await? else {
            let affected = if columns.is_empty() {
                source.rows_affected().unwrap_or_default()
            } else {
                0
            };
            return Ok(RawRows::Postgres {
                columns,
                rows: Vec::new(),
                affected,
            });
        };
        if columns.is_empty() {
            self.retire |= first
                .columns()
                .iter()
                .any(|c| Type::from_oid(c.type_().oid()).is_none());
            columns = first
                .columns()
                .iter()
                .map(crate::metadata::postgres)
                .collect();
        }
        let rows = if first
            .columns()
            .iter()
            .any(|column| text::server_text(column.type_()))
        {
            // Session-sensitive values need a second query to format them. Until the
            // first query is consumed, its bounded response channel can block that query.
            let mut buffered = vec![first];
            while let Some(row) = next(&mut source, &query).await? {
                buffered.push(row);
            }
            let mut formatted = VecDeque::with_capacity(buffered.len());
            for row in buffered {
                let text = text::convert(&self.client, &row, &query).await?;
                formatted.push_back(Row { binary: row, text });
            }
            Pending::Formatted(formatted)
        } else {
            Pending::Stream {
                first: Some(first),
                source,
                query,
            }
        };
        self.lazy = Some(Active {
            owner: Arc::downgrade(owner),
            rows,
        });
        Ok(RawRows::Postgres {
            columns,
            rows: Vec::new(),
            affected: 1,
        })
    }

    pub async fn read_lazy(
        &mut self,
        owner: &Arc<()>,
        all: bool,
    ) -> Result<Option<(RawRows, bool)>, DbError> {
        let Some(active) = self.lazy.as_mut().filter(|lazy| lazy.owns(owner)) else {
            return Ok(None);
        };
        let mut rows = Vec::new();
        let done = loop {
            match active.rows.next().await {
                Ok(Some(row)) => rows.push(row),
                Ok(None) => break true,
                Err(error) => {
                    self.discard_lazy();
                    return Err(error);
                }
            }
            if !all {
                break false;
            }
        };
        if done {
            self.discard_lazy();
        }
        Ok(Some((
            RawRows::Postgres {
                columns: Vec::new(),
                rows,
                affected: 0,
            },
            done,
        )))
    }
}

impl Active {
    fn owns(&self, owner: &Arc<()>) -> bool {
        Weak::ptr_eq(&self.owner, &Arc::downgrade(owner))
    }
}

impl Pending {
    async fn next(&mut self) -> Result<Option<Row>, DbError> {
        match self {
            Self::Formatted(rows) => Ok(rows.pop_front()),
            Self::Stream {
                first,
                source,
                query,
            } => {
                let row = match first.take() {
                    Some(row) => Some(row),
                    None => next(source, query).await?,
                };
                Ok(row.map(|binary| Row {
                    binary,
                    text: Vec::new(),
                }))
            }
        }
    }
}

async fn next(
    source: &mut Pin<Box<RowStream>>,
    query: &str,
) -> Result<Option<tokio_postgres::Row>, DbError> {
    poll_fn(|context| source.as_mut().poll_next(context))
        .await
        .transpose()
        .map_err(|failure| error(failure, query))
}
