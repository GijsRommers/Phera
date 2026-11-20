//! Query messages and connection state shared by PDO and pgsql.

use std::collections::VecDeque;
use std::io;

use bytes::{BufMut, BytesMut};
use postgres_protocol::IsNull;
use postgres_protocol::message::{
    backend::{ErrorResponseBody, Message},
    frontend::{self, BindError},
};

use super::{MAX_MESSAGE, Transport, ready_status, server_error, startup::Session};
use crate::drivers::connection_error;
use crate::errors::DbError;
use crate::parameters::BoundValue;

const ASYNC_MESSAGES: usize = 4096;

pub(in super::super) struct Client {
    transport: Transport,
    session: Session,
    notices: VecDeque<String>,
    notifications: VecDeque<Notification>,
}

pub struct Notification {
    pid: i32,
    channel: String,
    payload: String,
}

impl Notification {
    pub fn process_id(&self) -> i32 {
        self.pid
    }
    pub fn channel(&self) -> &str {
        &self.channel
    }
    pub fn payload(&self) -> &str {
        &self.payload
    }
}

impl Client {
    pub(in super::super) fn new(transport: Transport, session: Session) -> Self {
        Self {
            transport,
            session,
            notices: VecDeque::new(),
            notifications: VecDeque::new(),
        }
    }

    pub(in super::super) fn is_closed(&self) -> bool {
        self.transport.is_broken()
    }

    pub(in super::super) fn pid(&self) -> i32 {
        self.session.pid
    }
    pub(in super::super) fn status(&self) -> u8 {
        self.session.status
    }

    pub(in super::super) fn parameter(&self, name: &str) -> Option<&str> {
        self.session.parameters.get(name).map(String::as_str)
    }

    pub(in super::super) fn notices(&mut self, clear: bool) -> Vec<String> {
        if clear {
            self.notices.clear();
        }
        self.notices.iter().cloned().collect()
    }

    pub(in super::super) fn notification(&mut self) -> Option<Notification> {
        self.notifications.pop_front()
    }

    pub(in super::super) fn clear_messages(&mut self) {
        self.notices.clear();
        self.notifications.clear();
    }

    pub(in super::super) fn invalid(&mut self, cause: impl std::fmt::Display) -> DbError {
        self.transport.retire();
        connection_error(cause)
    }

    pub(in super::super) async fn send(
        &mut self,
        encode: impl FnOnce(&mut BytesMut) -> io::Result<()>,
    ) -> Result<(), DbError> {
        self.transport.send(encode).await.map_err(connection_error)
    }

    pub(in super::super) async fn simple(&mut self, sql: &str) -> Result<(), DbError> {
        if sql.len() > MAX_MESSAGE - 5 {
            return Err(DbError::general(
                "PostgreSQL query exceeds the 64 MiB limit",
            ));
        }
        self.send(|out| frontend::query(sql, out)).await
    }

    /// Parse and execute together on first use. A Describe of the portal
    /// supplies the result metadata in the same exchange as the text rows.
    pub(in super::super) async fn extended(
        &mut self,
        name: &str,
        sql: Option<&str>,
        values: &[BoundValue],
        close: &[String],
    ) -> Result<(), DbError> {
        check_bind(name, sql, values)?;
        self.send(|out| {
            for name in close {
                frontend::close(b'S', name, out)?;
            }
            if let Some(sql) = sql {
                frontend::parse(name, sql, [], out)?;
            }
            frontend::bind("", name, [0], values, text_parameter, [0], out).map_err(|error| {
                match error {
                    BindError::Conversion(error) => io::Error::other(error),
                    BindError::Serialization(error) => error,
                }
            })?;
            frontend::describe(b'P', "", out)?;
            frontend::execute("", 0, out)?;
            frontend::sync(out);
            Ok(())
        })
        .await
    }

    pub(in super::super) async fn prepare(&mut self, name: &str, sql: &str) -> Result<(), DbError> {
        check_bind(name, Some(sql), &[])?;
        self.send(|out| {
            frontend::parse(name, sql, [], out)?;
            frontend::sync(out);
            Ok(())
        })
        .await
    }

    /// Delivers protocol responses in order. Asynchronous messages update the
    /// session whether they occur between queries or inside a result.
    pub(in super::super) async fn next(&mut self) -> Result<Message, DbError> {
        loop {
            let message = self.transport.receive().await.map_err(connection_error)?;
            if let Some(message) = self.observe(message).map_err(|cause| self.invalid(cause))? {
                return Ok(message);
            }
        }
    }

    fn observe(&mut self, message: Message) -> io::Result<Option<Message>> {
        match message {
            Message::ParameterStatus(parameter) => {
                self.session
                    .parameters
                    .insert(parameter.name()?.into(), parameter.value()?.into());
            }
            Message::NoticeResponse(notice) => {
                bounded_push(
                    &mut self.notices,
                    server_error(notice.fields(), "", false)?.detail,
                );
            }
            Message::NotificationResponse(notice) => bounded_push(
                &mut self.notifications,
                Notification {
                    pid: notice.process_id(),
                    channel: notice.channel()?.into(),
                    payload: notice.message()?.into(),
                },
            ),
            Message::ReadyForQuery(ref ready) => {
                self.session.status = ready_status(ready.status())?;
                return Ok(Some(message));
            }
            message => return Ok(Some(message)),
        }
        Ok(None)
    }

    pub(in super::super) async fn consume_input(&mut self) -> Result<(), DbError> {
        while let Some(message) = self
            .transport
            .try_receive()
            .await
            .map_err(connection_error)?
        {
            if self
                .observe(message)
                .map_err(|cause| self.invalid(cause))?
                .is_some()
            {
                return Err(self.invalid("unexpected PostgreSQL response while idle"));
            }
        }
        Ok(())
    }

    pub(in super::super) async fn failure(
        &mut self,
        body: ErrorResponseBody,
        sql: &str,
        context: bool,
    ) -> DbError {
        let error = match server_error(body.fields(), sql, context) {
            Ok(error) => error,
            Err(cause) => return self.invalid(cause),
        };
        loop {
            match self.next().await {
                Ok(Message::ReadyForQuery(_)) => return error,
                Ok(_) => {}
                Err(failure) => return failure,
            }
        }
    }
}

fn bounded_push<T>(queue: &mut VecDeque<T>, item: T) {
    if queue.len() == ASYNC_MESSAGES {
        queue.pop_front();
    }
    queue.push_back(item);
}

fn check_bind(name: &str, sql: Option<&str>, values: &[BoundValue]) -> Result<(), DbError> {
    // Parse and Bind each have their own limit. Check before an encoder
    // reserves output for attacker-controlled PHP string lengths.
    let parse = name
        .len()
        .checked_add(sql.map_or(0, str::len))
        .and_then(|size| size.checked_add(8));
    let bind = name.len().checked_add(16).and_then(|size| {
        values.iter().try_fold(size, |size, value| {
            let length = match value {
                BoundValue::String(bytes) | BoundValue::Blob(bytes) => bytes.len(),
                BoundValue::Int(_) => 20,
                BoundValue::Bool(_) => 1,
                BoundValue::Null => 0,
            };
            size.checked_add(4)?.checked_add(length)
        })
    });
    if values.len() > usize::from(u16::MAX)
        || parse.is_none_or(|size| size > MAX_MESSAGE)
        || bind.is_none_or(|size| size > MAX_MESSAGE)
    {
        return Err(DbError::general(
            "PostgreSQL Parse or Bind exceeds the protocol limits",
        ));
    }
    Ok(())
}

fn text_parameter(
    value: &BoundValue,
    out: &mut BytesMut,
) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
    match value {
        BoundValue::Null => return Ok(IsNull::Yes),
        BoundValue::String(bytes) | BoundValue::Blob(bytes) => out.extend_from_slice(bytes),
        BoundValue::Int(value) => out.extend_from_slice(value.to_string().as_bytes()),
        BoundValue::Bool(value) => out.put_u8(if *value { b't' } else { b'f' }),
    }
    Ok(IsNull::No)
}
