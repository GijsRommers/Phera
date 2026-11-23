//! `curl_setopt($handle, CURLOPT_INFILE, $input)` streams a PHP resource to Hyper.

mod frames;
mod multipart;

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use bytes::Bytes;
use http_body_util::Full;
use phera_extension_api::{Context, NativeError, Value};
use tokio::sync::oneshot;

use crate::{
    error::TransferError,
    options::{Body, Method, Settings},
};

use super::{RequestBody, Sender, Transfer};
use frames::Segment;

#[derive(Default)]
pub(super) struct Payload {
    content: Content,
    pub(super) length: Option<u64>,
    pub(super) content_type: Option<String>,
    pub(super) has_body: bool,
}

#[derive(Default)]
enum Content {
    #[default]
    Empty,
    Bytes(Bytes),
    Files(Arc<[Segment]>),
    Input,
}

pub(super) type Sent = Arc<AtomicU64>;

impl Payload {
    pub(super) fn request_body(
        &self,
        sender: &Sender,
        uploaded: i64,
    ) -> Result<(RequestBody, Option<Sent>), TransferError> {
        let sent;
        let frames = match &self.content {
            Content::Empty => return Ok((RequestBody::Full(Full::new(Bytes::new())), None)),
            Content::Bytes(data) => return Ok((RequestBody::Full(Full::new(data.clone())), None)),
            Content::Files(segments) => {
                sent = Sent::default();
                frames::files(Arc::clone(segments), Arc::clone(&sent))
            }
            Content::Input => {
                if uploaded > 0 {
                    return Err(TransferError::new(
                        65,
                        "necessary data rewind was not possible",
                    ));
                }
                sent = Sent::default();
                frames::input(sender.clone(), self.length, Arc::clone(&sent))
            }
        };
        Ok((
            RequestBody::Stream {
                frames,
                remaining: self.length,
            },
            Some(sent),
        ))
    }

    pub(super) fn sent(&self, counter: Option<&Sent>, received_response: bool) -> i64 {
        let count = counter.map_or_else(
            || self.length.filter(|_| received_response).unwrap_or(0),
            |sent| sent.load(Ordering::Relaxed),
        );
        i64::try_from(count).unwrap_or(i64::MAX)
    }
}

pub(super) struct ReadRequest {
    pub(super) limit: u64,
    pub(super) length: Option<i64>,
    pub(super) sent: i64,
    pub(super) reply: oneshot::Sender<Result<Bytes, TransferError>>,
}

pub(super) async fn build(
    context: &mut dyn Context,
    settings: &Settings,
) -> Result<Result<Payload, TransferError>, NativeError> {
    let payload = match (settings.method, &settings.body) {
        (Method::Post, Some(Body::Text(bytes))) => Payload {
            content: Content::Bytes(Bytes::copy_from_slice(bytes)),
            length: Some(bytes.len() as u64),
            content_type: Some("application/x-www-form-urlencoded".to_owned()),
            has_body: true,
        },
        (Method::Post, Some(Body::Multipart(fields))) => {
            return multipart::build(context, fields).await;
        }
        (Method::Put | Method::Post, _) => {
            let (content, length) = if settings.read.is_some() || settings.input.is_some() {
                (Content::Input, settings.input_size)
            } else {
                (Content::Empty, Some(0))
            };
            Payload {
                content,
                length,
                content_type: (settings.method == Method::Post)
                    .then(|| "application/x-www-form-urlencoded".to_owned()),
                has_body: true,
            }
        }
        _ => Payload::default(),
    };
    Ok(Ok(payload))
}

pub(super) async fn respond(
    context: &mut dyn Context,
    transfer: &Transfer,
    request: ReadRequest,
) -> Result<(), NativeError> {
    let read = read_chunk(context, transfer, &request);
    let result = if let Some(timeout) = transfer.settings.timeout {
        let remaining = timeout.saturating_sub(transfer.started.elapsed());
        match tokio::time::timeout(remaining, read).await {
            Ok(result) => result?,
            Err(_) => Err(TransferError::new(
                28,
                format!(
                    "Operation timed out after {} milliseconds with 0 bytes received",
                    transfer.started.elapsed().as_millis(),
                ),
            )),
        }
    } else {
        read.await?
    };
    let _ = request.reply.send(result);
    Ok(())
}

async fn read_chunk(
    context: &mut dyn Context,
    transfer: &Transfer,
    request: &ReadRequest,
) -> Result<Result<Bytes, TransferError>, NativeError> {
    let settings = &transfer.settings;
    if let Some(read) = &settings.read {
        let input = settings
            .input
            .as_ref()
            .map_or(Value::Null, |input| Value::Resource(input.clone()));
        let value = context
            .call(
                read,
                vec![
                    Value::Object(transfer.object.clone()),
                    input,
                    Value::Int(request.limit as i64),
                ],
            )
            .await?;
        return Ok(match &*value.deref() {
            Value::InternedStr(text) if text.as_bytes().len() as u64 <= request.limit => {
                Ok(Bytes::copy_from_slice(text.as_bytes()))
            }
            Value::String(text) if text.as_bytes().len() as u64 <= request.limit => {
                Ok(Bytes::copy_from_slice(text.as_bytes()))
            }
            Value::InternedStr(_) | Value::String(_) => Err(TransferError::new(
                26,
                "read function returned too much data",
            )),
            Value::Int(268435456) => Err(TransferError::new(42, "operation aborted by callback")),
            _ => Ok(Bytes::new()),
        });
    }
    if let Some(input) = &settings.input {
        return Ok(
            phera_ext_streams::native::read_resource(input, request.limit)
                .await?
                .map(Bytes::from)
                .map_err(|_| TransferError::new(26, "Failed to read upload data")),
        );
    }
    Ok(Ok(Bytes::new()))
}
