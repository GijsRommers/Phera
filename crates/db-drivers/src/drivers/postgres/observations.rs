//! libpq result status and field type metadata from PostgreSQL response headers.

use std::collections::VecDeque;
use std::future::poll_fn;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use bytes::BytesMut;
use postgres_protocol::message::backend::Message;
use tokio_postgres::{AsyncMessage, Connection, Notification, tls::NoTlsStream};

use super::native::NativeColumn;
use super::stream::ObservedStream;

#[derive(Default)]
pub struct Observations {
    capture: AtomicBool,
    pid: AtomicI32,
    replies: Mutex<VecDeque<Header>>,
    notices: Mutex<VecDeque<String>>,
    notifications: Mutex<VecDeque<Notification>>,
}

pub enum Header {
    Columns(Vec<NativeColumn>),
    Command(String),
}

impl Observations {
    pub fn is_capturing(&self) -> bool {
        self.capture.load(Ordering::Relaxed)
    }

    pub fn interested(&self, kind: u8) -> bool {
        kind == b'K' || self.capture.load(Ordering::Relaxed) && matches!(kind, b'T' | b'C')
    }

    pub fn capture(&self) {
        self.replies
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clear();
        self.capture.store(true, Ordering::Relaxed);
    }

    pub fn finish(&self) -> VecDeque<Header> {
        self.capture.store(false, Ordering::Relaxed);
        std::mem::take(
            &mut *self
                .replies
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()),
        )
    }

    pub fn next(&self) -> Option<Header> {
        self.replies
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .pop_front()
    }

    pub fn pid(&self) -> i32 {
        self.pid.load(Ordering::Relaxed)
    }

    pub fn record(&self, packet: &mut BytesMut) {
        let header = match Message::parse(packet) {
            Ok(Some(Message::BackendKeyData(body))) => {
                self.pid.store(body.process_id(), Ordering::Relaxed);
                return;
            }
            Ok(Some(Message::CommandComplete(body))) => {
                body.tag().ok().map(|tag| Header::Command(tag.to_owned()))
            }
            Ok(Some(Message::RowDescription(body))) => {
                use fallible_iterator::FallibleIterator;
                let fields = body
                    .fields()
                    .map(|field| {
                        Ok(NativeColumn {
                            name: field.name().into(),
                            table: field.table_oid(),
                            oid: field.type_oid(),
                            size: field.type_size(),
                            kind: NativeColumn::kind(field.type_oid()),
                        })
                    })
                    .collect();
                fields.ok().map(Header::Columns)
            }
            _ => None,
        };
        if let Some(header) = header {
            self.replies
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .push_back(header);
        }
    }

    pub async fn listen(
        mut connection: Connection<ObservedStream, NoTlsStream>,
        observations: &Self,
    ) {
        while let Some(Ok(message)) = poll_fn(|cx| connection.poll_message(cx)).await {
            match message {
                AsyncMessage::Notice(notice) => push(
                    &observations.notices,
                    format!("{}:  {}", notice.severity(), notice.message()),
                ),
                AsyncMessage::Notification(notification) => {
                    push(&observations.notifications, notification)
                }
                _ => {}
            }
        }
    }

    pub fn notices(&self, clear: bool) -> Vec<String> {
        let mut list = self
            .notices
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if clear {
            list.clear();
        }
        list.iter().cloned().collect()
    }

    pub fn notification(&self) -> Option<Notification> {
        self.notifications
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .pop_front()
    }

    pub fn reset(&self) {
        self.finish();
        self.notices(true);
        self.notifications
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clear();
    }
}

fn push<T>(queue: &Mutex<VecDeque<T>>, value: T) {
    let mut queue = queue.lock().unwrap_or_else(|poison| poison.into_inner());
    // Long-lived listeners may receive notifications without reading them.
    if queue.len() >= 4096 {
        queue.pop_front();
    }
    queue.push_back(value);
}
