//! Moving gRPC frames over one HTTP/2 stream, shared by server and client.

use std::future::poll_fn;

use bytes::Bytes;
use h2::{RecvStream, SendStream};

use crate::framing::{Deframer, Frame};
use crate::status::{CANCELLED, INTERNAL, Status};

/// Sends `data`, waiting while the peer's flow-control window is full, so
/// a slow reader holds the sender back instead of filling memory.
pub async fn send(
    stream: &mut SendStream<Bytes>,
    mut data: Bytes,
    end_of_stream: bool,
) -> Result<(), h2::Error> {
    if data.is_empty() {
        return stream.send_data(data, end_of_stream);
    }
    while !data.is_empty() {
        stream.reserve_capacity(data.len());
        if stream.capacity() == 0 {
            match poll_fn(|cx| stream.poll_capacity(cx)).await {
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(error),
                // The stream was closed or reset by the peer.
                None => return Err(h2::Error::from(h2::Reason::CANCEL)),
            }
        }
        let granted = stream.capacity().min(data.len());
        if granted == 0 {
            continue;
        }
        let chunk = data.split_to(granted);
        stream.send_data(chunk, end_of_stream && data.is_empty())?;
    }
    Ok(())
}

/// Reads a stream's DATA to its end and returns the frames in it.
pub async fn read_frames(body: &mut RecvStream, max_message: usize) -> Result<Vec<Frame>, Status> {
    let mut deframer = Deframer::new(max_message);
    while let Some(chunk) = body.data().await {
        let chunk = chunk.map_err(stream_error)?;
        // Let the peer send as much again.
        let _ = body.flow_control().release_capacity(chunk.len());
        deframer.push(chunk)?;
    }
    deframer.finish()
}

/// What a broken stream means for the call on it.
pub fn stream_error(error: h2::Error) -> Status {
    if error.reason() == Some(h2::Reason::CANCEL) {
        Status::new(CANCELLED, "the call was cancelled")
    } else {
        Status::new(INTERNAL, format!("HTTP/2 error: {error}"))
    }
}
