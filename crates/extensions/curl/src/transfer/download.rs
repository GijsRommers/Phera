//! `CURLOPT_ACCEPT_ENCODING` enables incremental gzip, deflate and Brotli decoding.

use std::{cell::Cell, io, pin::Pin, rc::Rc, time::Duration};

use async_compression::tokio::bufread::{BrotliDecoder, DeflateDecoder, GzipDecoder, ZlibDecoder};
use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt};
use tokio::time::Instant;
use tokio_util::io::StreamReader;

use crate::{error::TransferError, options::Settings};

use super::{Event, Sender};

pub(super) async fn deliver(
    body: Incoming,
    encoding: Option<&str>,
    settings: &Settings,
    sender: &Sender,
    wire_bytes: &Rc<Cell<i64>>,
    discard: bool,
) -> Result<(), TransferError> {
    let body = body.into_data_stream().map({
        let wire_bytes = wire_bytes.clone();
        move |chunk| {
            chunk
                .inspect(|bytes| {
                    wire_bytes.set(wire_bytes.get().saturating_add(bytes.len() as i64));
                })
                .map_err(io::Error::other)
        }
    });
    let mut body = StreamReader::new(body);
    let encoding = encoding
        .filter(|value| *value != "identity")
        .filter(|_| !discard && settings.encoding.is_some() && settings.decode);
    let mut reader: Pin<Box<dyn AsyncRead>> = match encoding {
        None => Box::pin(body),
        Some("gzip" | "x-gzip") => Box::pin(GzipDecoder::new(body)),
        Some("br") => Box::pin(BrotliDecoder::new(body)),
        Some("deflate") => {
            let header = body.fill_buf().await.map_err(read_error)?;
            let zlib = header.len() < 2
                || (header[0] & 15 == 8 && u16::from_be_bytes([header[0], header[1]]) % 31 == 0);
            if zlib {
                Box::pin(ZlibDecoder::new(body))
            } else {
                Box::pin(DeflateDecoder::new(body))
            }
        }
        Some(_) => {
            return Err(TransferError::new(
                61,
                concat!(
                    "Unrecognized content encoding type. ",
                    "libcurl understands deflate, gzip content encodings."
                ),
            ));
        }
    };
    let mut buffer = vec![0; settings.buffer_size];
    let mut speed = LowSpeed::new(settings);
    let mut delivered = 0u64;
    loop {
        let count = match speed.deadline {
            Some(deadline) => tokio::time::timeout_at(deadline, reader.read(&mut buffer))
                .await
                .map_err(|_| speed.error())?,
            None => reader.read(&mut buffer).await,
        }
        .map_err(|error| {
            if encoding.is_some()
                && error
                    .get_ref()
                    .and_then(|e| e.downcast_ref::<hyper::Error>())
                    .is_none()
            {
                TransferError::new(61, format!("Content decoding failed: {error}"))
            } else {
                read_error(error)
            }
        })?;
        if count == 0 {
            return Ok(());
        }
        speed.advance(wire_bytes.get() as u64);
        delivered += count as u64;
        if !discard && settings.max_file_size != 0 && delivered > settings.max_file_size {
            return Err(TransferError::new(63, "Maximum file size exceeded"));
        }
        if !discard
            && sender
                .send(Event::Body(
                    Bytes::copy_from_slice(&buffer[..count]),
                    wire_bytes.get(),
                ))
                .await
                .is_err()
        {
            return Ok(());
        }
    }
}

fn read_error(error: io::Error) -> TransferError {
    let hyper = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<hyper::Error>());
    if hyper.is_some_and(hyper::Error::is_body_write_aborted)
        || error.kind() == io::ErrorKind::UnexpectedEof
    {
        TransferError::new(18, "transfer closed with outstanding read data remaining")
    } else {
        TransferError::new(
            56,
            format!("Failure when receiving data from the peer: {error}"),
        )
    }
}

pub(super) struct LowSpeed {
    deadline: Option<Instant>,
    interval: Duration,
    threshold: u64,
    minimum: u64,
    initial: u64,
}

impl LowSpeed {
    pub(super) fn new(settings: &Settings) -> Self {
        let interval = settings.low_speed_time;
        let minimum = settings.low_speed_limit;
        Self {
            deadline: if minimum > 0 && !interval.is_zero() {
                Instant::now().checked_add(interval)
            } else {
                None
            },
            interval,
            threshold: minimum.saturating_mul(interval.as_secs()),
            minimum,
            initial: 0,
        }
    }

    pub(super) fn advance(&mut self, bytes: u64) -> bool {
        if self.deadline.is_some() && bytes.saturating_sub(self.initial) >= self.threshold {
            self.initial = bytes;
            self.deadline = Instant::now().checked_add(self.interval);
            return true;
        }
        false
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub(super) fn error(&self) -> TransferError {
        TransferError::new(
            28,
            format!(
                "Operation too slow. Less than {} bytes/sec transferred the last {} seconds",
                self.minimum,
                self.interval.as_secs()
            ),
        )
    }
}
