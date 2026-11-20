//! `PDO::ATTR_PREFETCH => false` keeps reading the same socket at fetch time.

use std::collections::VecDeque;
use std::sync::{Arc, Weak};

use postgres_protocol::message::backend::{DataRowBody, Message};

use super::{Postgres, values};
use crate::drivers::{Column, RowBuffer, Rows};
use crate::errors::DbError;

pub(super) struct Active {
    owner: Weak<()>,
    columns: Vec<Column>,
    query: String,
    rows: Pending,
}

enum Pending {
    Socket(Option<DataRowBody>),
    Buffered(VecDeque<Result<DataRowBody, DbError>>),
}

impl Active {
    pub(super) fn new(
        owner: &Arc<()>,
        columns: Vec<Column>,
        query: String,
        first: DataRowBody,
    ) -> Self {
        Self {
            owner: Arc::downgrade(owner),
            columns,
            query,
            rows: Pending::Socket(Some(first)),
        }
    }

    fn owns(&self, owner: &Arc<()>) -> bool {
        self.owner.ptr_eq(&Arc::downgrade(owner))
    }
}

impl Postgres {
    pub async fn discard_lazy(&mut self) -> Result<(), DbError> {
        if let Some(Active {
            rows: Pending::Socket(_),
            query,
            ..
        }) = self.lazy.take()
        {
            self.finish(&query).await?;
        }
        Ok(())
    }

    pub async fn close_lazy(&mut self, owner: &Arc<()>) -> Result<(), DbError> {
        if self.lazy.as_ref().is_some_and(|lazy| lazy.owns(owner)) {
            self.discard_lazy().await?;
        }
        Ok(())
    }

    /// A side query uses the same socket, so save the unread cells first.
    pub async fn buffer_lazy(&mut self) {
        let Some(mut active) = self.lazy.take() else {
            return;
        };
        if let Pending::Socket(first) = &mut active.rows {
            let mut rows = VecDeque::new();
            if let Some(first) = first.take() {
                rows.push_back(Ok(first));
            }
            loop {
                match self.lazy_row(&active.query).await {
                    Ok(Some(row)) => rows.push_back(Ok(row)),
                    Ok(None) => break,
                    Err(error) => {
                        rows.push_back(Err(error));
                        break;
                    }
                }
            }
            active.rows = Pending::Buffered(rows);
        }
        self.lazy = Some(active);
    }

    pub async fn read_lazy(
        &mut self,
        owner: &Arc<()>,
        all: bool,
    ) -> Result<Option<(Rows, bool)>, DbError> {
        let Some(mut active) = self.lazy.take() else {
            return Ok(None);
        };
        if !active.owns(owner) {
            self.lazy = Some(active);
            return Ok(None);
        }
        let mut values = RowBuffer::with_capacity(active.columns.len(), usize::from(!all));
        let done = loop {
            let streaming = matches!(active.rows, Pending::Socket(_));
            let row = match &mut active.rows {
                Pending::Socket(first) => match first.take() {
                    Some(row) => Some(row),
                    None => self.lazy_row(&active.query).await?,
                },
                Pending::Buffered(rows) => rows.pop_front().transpose()?,
            };
            let Some(row) = row else {
                break true;
            };
            if let Err(error) = values::append(&active.columns, &mut values, &row) {
                if error.state == "08006" {
                    return Err(self.client.invalid(error.detail));
                }
                if streaming {
                    let _ = self.finish(&active.query).await;
                }
                return Err(error);
            }
            if !all {
                break false;
            }
        };
        if !done {
            self.lazy = Some(active);
        }
        Ok(Some((
            Rows {
                values,
                ..Rows::default()
            },
            done,
        )))
    }

    async fn lazy_row(&mut self, query: &str) -> Result<Option<DataRowBody>, DbError> {
        loop {
            match self.client.next().await? {
                Message::DataRow(row) => return Ok(Some(row)),
                Message::CommandComplete(_) => {
                    self.finish_after_result(query).await?;
                    return Ok(None);
                }
                Message::CloseComplete => {}
                Message::ReadyForQuery(_) => return Ok(None),
                Message::ErrorResponse(error) => {
                    return Err(self.client.failure(error, query, false).await);
                }
                _ => return Err(self.client.invalid("unexpected PostgreSQL cursor response")),
            }
        }
    }
}
