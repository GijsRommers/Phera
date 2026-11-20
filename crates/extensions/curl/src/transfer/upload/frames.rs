//! A `new CURLFile($path)` part is opened and read as Hyper sends its frames.

use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};

use bytes::Bytes;
use futures_util::stream;
use hyper::body::Frame;
use tokio::{fs::File, io::AsyncReadExt, sync::oneshot};

use crate::error::TransferError;

use super::{
    super::{Event, Sender, body::Frames},
    ReadRequest, Sent,
};

const CHUNK: u64 = 64 * 1024;

pub(super) enum Segment {
    Bytes(Bytes),
    File { path: PathBuf, length: u64 },
}

impl Segment {
    pub(super) fn length(&self) -> u64 {
        match self {
            Self::Bytes(bytes) => bytes.len() as u64,
            Self::File { length, .. } => *length,
        }
    }
}

pub(super) fn file_error() -> TransferError {
    TransferError::new(26, "Failed to open/read local data from file/application")
}

struct Files {
    segments: Arc<[Segment]>,
    next: usize,
    file: Option<(File, u64)>,
    sent: Sent,
}

pub(super) fn files(segments: Arc<[Segment]>, sent: Sent) -> Frames {
    let files = Files {
        segments,
        next: 0,
        file: None,
        sent,
    };
    Box::pin(stream::try_unfold(files, |mut files| async move {
        let Some(bytes) = files.next_chunk().await? else {
            return Ok(None);
        };
        files.sent.fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Ok(Some((Frame::data(bytes), files)))
    }))
}

impl Files {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, TransferError> {
        loop {
            if let Some((file, remaining)) = &mut self.file {
                let mut bytes = vec![0; (*remaining).min(CHUNK) as usize];
                let read = file.read(&mut bytes).await.map_err(|_| file_error())?;
                if read == 0 {
                    return Err(file_error());
                }
                *remaining -= read as u64;
                if *remaining == 0 {
                    self.file = None;
                }
                bytes.truncate(read);
                return Ok(Some(bytes.into()));
            }
            let Some(segment) = self.segments.get(self.next) else {
                return Ok(None);
            };
            self.next += 1;
            match segment {
                Segment::Bytes(bytes) if !bytes.is_empty() => return Ok(Some(bytes.clone())),
                Segment::File { path, length } if *length > 0 => {
                    let file = File::open(path).await.map_err(|_| file_error())?;
                    self.file = Some((file, *length));
                }
                _ => {}
            }
        }
    }
}

struct Input {
    sender: Sender,
    length: Option<u64>,
    sent: Sent,
}

pub(super) fn input(sender: Sender, length: Option<u64>, sent: Sent) -> Frames {
    let input = Input {
        sender,
        length,
        sent,
    };
    Box::pin(stream::try_unfold(input, |mut input| async move {
        let Some(bytes) = input.next_chunk().await? else {
            return Ok(None);
        };
        input.sent.fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Ok(Some((Frame::data(bytes), input)))
    }))
}

impl Input {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, TransferError> {
        let sent = self.sent.load(Ordering::Relaxed);
        let limit = self
            .length
            .map_or(CHUNK, |length| (length - sent).min(CHUNK));
        if limit == 0 {
            return Ok(None);
        }
        let (reply, receive) = oneshot::channel();
        let length = self.length.and_then(|length| i64::try_from(length).ok());
        self.sender
            .send(Event::Upload(ReadRequest {
                limit,
                length,
                sent: i64::try_from(sent).unwrap_or(i64::MAX),
                reply,
            }))
            .await
            .map_err(|_| interrupted())?;
        let bytes = receive.await.map_err(|_| interrupted())??;
        if !bytes.is_empty() {
            return Ok(Some(bytes));
        }
        if let Some(length) = self.length {
            return Err(TransferError::new(
                26,
                format!(
                    "client read function EOF fail, only {sent}/{length} \
                    of needed bytes read"
                ),
            ));
        }
        Ok(None)
    }
}

fn interrupted() -> TransferError {
    TransferError::new(42, "Upload was interrupted")
}
