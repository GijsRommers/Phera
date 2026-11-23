//! Hyper consumes upload frames as the connection has space for them.

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_util::Stream;
use http_body_util::Full;
use hyper::body::{Body, Frame, SizeHint};

use crate::error::TransferError;

pub(super) type Frames = Pin<Box<dyn Stream<Item = Result<Frame<Bytes>, TransferError>> + Send>>;

pub(crate) enum RequestBody {
    Full(Full<Bytes>),
    Stream {
        frames: Frames,
        remaining: Option<u64>,
    },
}

impl Body for RequestBody {
    type Data = Bytes;
    type Error = TransferError;

    fn poll_frame(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, TransferError>>> {
        match self.get_mut() {
            Self::Full(body) => Pin::new(body)
                .poll_frame(context)
                .map(|frame| frame.map(|frame| frame.map_err(|error| match error {}))),
            Self::Stream { frames, remaining } => {
                let frame = std::task::ready!(frames.as_mut().poll_next(context));
                if let Some(remaining) = remaining
                    && let Some(Ok(data)) = &frame
                    && let Some(bytes) = data.data_ref()
                {
                    *remaining = remaining.saturating_sub(bytes.len() as u64);
                }
                Poll::Ready(frame)
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            Self::Full(body) => body.is_end_stream(),
            Self::Stream { remaining, .. } => *remaining == Some(0),
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            Self::Full(body) => body.size_hint(),
            Self::Stream { remaining, .. } => {
                remaining.map_or_else(SizeHint::new, SizeHint::with_exact)
            }
        }
    }
}
