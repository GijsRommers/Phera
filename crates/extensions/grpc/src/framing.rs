//! gRPC message frames inside HTTP/2 DATA: a compressed flag, a big-endian
//! length, and the message.
//!
//! ```text
//! 00 | 00 00 00 04 | 0a 02 68 69
//! |    |             '- the protobuf message
//! |    '------------- length, 4 bytes, big-endian
//! '------------------ 1 when compressed with the call's grpc-encoding
//! ```
//!
//! A message is encoded straight after five reserved header bytes, and the
//! header is filled in afterwards, so a frame is one buffer that HTTP/2
//! sends as is. Incoming frames are cut out of the DATA chunks without a
//! copy when a chunk holds whole frames, which is the usual case.

use std::io::{Read, Write};

use bytes::{Bytes, BytesMut};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

use crate::status::{INTERNAL, RESOURCE_EXHAUSTED, Status, UNIMPLEMENTED};

pub const HEADER_LEN: usize = 5;

/// gRPC's default limit for a received message.
pub const DEFAULT_MAX_MESSAGE: usize = 4 * 1024 * 1024;

/// The `grpc-encoding` of a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Identity,
    Gzip,
}

impl Encoding {
    /// `None` for a compression this side does not have.
    pub fn parse(name: Option<&[u8]>) -> Option<Self> {
        match name {
            None | Some(b"identity") => Some(Self::Identity),
            Some(b"gzip") => Some(Self::Gzip),
            Some(_) => None,
        }
    }

    pub fn accepts_gzip(header: Option<&[u8]>) -> bool {
        header.is_some_and(|value| {
            value
                .split(|&byte| byte == b',')
                .any(|name| name.trim_ascii() == b"gzip")
        })
    }
}

/// A buffer that starts with the header space of one frame.
pub fn begin() -> Vec<u8> {
    let mut frame = Vec::with_capacity(64);
    frame.extend_from_slice(&[0; HEADER_LEN]);
    frame
}

/// Fills in the header of a buffer from [`begin`], compressing the message
/// first when `encoding` asks for it.
pub fn finish(mut frame: Vec<u8>, encoding: Encoding) -> Bytes {
    if encoding == Encoding::Gzip {
        let compressed = gzip(&frame[HEADER_LEN..]);
        frame.truncate(HEADER_LEN);
        frame.extend_from_slice(&compressed);
        frame[0] = 1;
    }
    let len = (frame.len() - HEADER_LEN) as u32;
    frame[1..HEADER_LEN].copy_from_slice(&len.to_be_bytes());
    Bytes::from(frame)
}

/// A frame around message bytes that are already encoded.
pub fn frame(message: &[u8], encoding: Encoding) -> Bytes {
    let mut frame = Vec::with_capacity(HEADER_LEN + message.len());
    frame.extend_from_slice(&[0; HEADER_LEN]);
    frame.extend_from_slice(message);
    finish(frame, encoding)
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(
        Vec::with_capacity(bytes.len() / 2 + 32),
        Compression::default(),
    );
    // Writing into a Vec cannot fail.
    let _ = encoder.write_all(bytes);
    encoder.finish().unwrap_or_default()
}

/// One received frame, still compressed if its flag says so.
#[derive(Debug)]
pub struct Frame {
    pub compressed: bool,
    pub message: Bytes,
}

impl Frame {
    /// The message bytes, decompressed with the call's encoding.
    pub fn into_message(self, encoding: Encoding, max: usize) -> Result<Bytes, Status> {
        if !self.compressed {
            return Ok(self.message);
        }
        if encoding == Encoding::Identity {
            return Err(Status::new(
                INTERNAL,
                "compressed message without a grpc-encoding",
            ));
        }
        let mut out = Vec::with_capacity(self.message.len() * 2);
        // One byte more than allowed tells an oversized message apart.
        let read = GzDecoder::new(&self.message[..])
            .take(max as u64 + 1)
            .read_to_end(&mut out);
        match read {
            Err(error) => Err(Status::new(
                INTERNAL,
                format!("invalid gzip message: {error}"),
            )),
            Ok(_) if out.len() > max => Err(too_large(out.len(), max)),
            Ok(_) => Ok(Bytes::from(out)),
        }
    }
}

fn too_large(len: usize, max: usize) -> Status {
    Status::new(
        RESOURCE_EXHAUSTED,
        format!("message of {len} bytes is larger than the limit of {max}"),
    )
}

/// Cuts frames out of a call's DATA chunks.
pub struct Deframer {
    partial: BytesMut,
    max: usize,
    frames: Vec<Frame>,
}

impl Deframer {
    pub fn new(max: usize) -> Self {
        Self {
            partial: BytesMut::new(),
            max,
            frames: Vec::new(),
        }
    }

    pub fn push(&mut self, mut chunk: Bytes) -> Result<(), Status> {
        if self.partial.is_empty() {
            // Whole frames are slices of the chunk itself.
            while chunk.len() >= HEADER_LEN {
                let len = self.length(&chunk)?;
                if chunk.len() < HEADER_LEN + len {
                    break;
                }
                let frame = chunk.split_to(HEADER_LEN + len);
                self.frames.push(Frame {
                    compressed: frame[0] == 1,
                    message: frame.slice(HEADER_LEN..),
                });
            }
            if !chunk.is_empty() {
                self.partial.extend_from_slice(&chunk);
                self.reserve_announced()?;
            }
            return Ok(());
        }
        self.partial.extend_from_slice(&chunk);
        while self.partial.len() >= HEADER_LEN {
            let len = self.length(&self.partial)?;
            if self.partial.len() < HEADER_LEN + len {
                break;
            }
            let frame = self.partial.split_to(HEADER_LEN + len).freeze();
            self.frames.push(Frame {
                compressed: frame[0] == 1,
                message: frame.slice(HEADER_LEN..),
            });
        }
        self.reserve_announced()
    }

    /// The remaining frames once the stream has ended.
    pub fn finish(mut self) -> Result<Vec<Frame>, Status> {
        if !self.partial.is_empty() {
            return Err(Status::new(INTERNAL, "the stream ended inside a message"));
        }
        Ok(std::mem::take(&mut self.frames))
    }

    /// Grows the buffer once for a frame whose header has arrived.
    fn reserve_announced(&mut self) -> Result<(), Status> {
        if self.partial.len() >= HEADER_LEN {
            let len = self.length(&self.partial)?;
            self.partial
                .reserve(HEADER_LEN + len - self.partial.len().min(HEADER_LEN + len));
        }
        Ok(())
    }

    fn length(&self, header: &[u8]) -> Result<usize, Status> {
        if header[0] > 1 {
            return Err(Status::new(
                INTERNAL,
                format!("invalid compressed flag {}", header[0]),
            ));
        }
        let len = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
        if len > self.max {
            return Err(too_large(len, self.max));
        }
        Ok(len)
    }
}

/// Status for a request whose `grpc-encoding` is not supported.
pub fn unsupported_encoding(name: &[u8]) -> Status {
    Status::new(
        UNIMPLEMENTED,
        format!(
            "grpc-encoding {} is not supported",
            String::from_utf8_lossy(name)
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_carry_flag_and_big_endian_length() {
        let framed = frame(b"\x0a\x02hi", Encoding::Identity);
        assert_eq!(&framed[..], b"\x00\x00\x00\x00\x04\x0a\x02hi");
    }

    #[test]
    fn frames_split_across_chunks_are_joined() {
        let one = frame(b"first", Encoding::Identity);
        let two = frame(b"second", Encoding::Identity);
        let mut wire = one.to_vec();
        wire.extend_from_slice(&two);
        for cut in 0..wire.len() {
            let mut deframer = Deframer::new(DEFAULT_MAX_MESSAGE);
            deframer.push(Bytes::copy_from_slice(&wire[..cut])).unwrap();
            deframer.push(Bytes::copy_from_slice(&wire[cut..])).unwrap();
            let messages: Vec<Bytes> = deframer
                .finish()
                .unwrap()
                .into_iter()
                .map(|frame| frame.message)
                .collect();
            assert_eq!(messages, [&b"first"[..], &b"second"[..]], "cut at {cut}");
        }
    }

    #[test]
    fn a_truncated_stream_and_an_oversized_frame_are_errors() {
        let mut deframer = Deframer::new(DEFAULT_MAX_MESSAGE);
        deframer
            .push(Bytes::from_static(b"\x00\x00\x00\x00\x09abc"))
            .unwrap();
        assert_eq!(deframer.finish().unwrap_err().code, INTERNAL);
        let mut deframer = Deframer::new(8);
        let error = deframer
            .push(Bytes::from_static(b"\x00\x00\x00\x00\x09"))
            .unwrap_err();
        assert_eq!(error.code, RESOURCE_EXHAUSTED);
    }

    #[test]
    fn gzip_frames_round_trip_and_respect_the_limit() {
        let message = vec![b'a'; 10_000];
        let framed = frame(&message, Encoding::Gzip);
        assert_eq!(framed[0], 1);
        assert!(framed.len() < 200, "compressed to {}", framed.len());
        let mut deframer = Deframer::new(DEFAULT_MAX_MESSAGE);
        deframer.push(framed.clone()).unwrap();
        let frame = deframer.finish().unwrap().pop().unwrap();
        let decoded = frame
            .into_message(Encoding::Gzip, DEFAULT_MAX_MESSAGE)
            .unwrap();
        assert_eq!(decoded, message);
        // The limit applies to the decompressed size.
        let mut deframer = Deframer::new(DEFAULT_MAX_MESSAGE);
        deframer.push(framed).unwrap();
        let frame = deframer.finish().unwrap().pop().unwrap();
        let error = frame.into_message(Encoding::Gzip, 1000).unwrap_err();
        assert_eq!(error.code, RESOURCE_EXHAUSTED);
    }

    #[test]
    fn encodings_are_negotiated_by_name() {
        assert_eq!(Encoding::parse(None), Some(Encoding::Identity));
        assert_eq!(Encoding::parse(Some(b"gzip")), Some(Encoding::Gzip));
        assert_eq!(Encoding::parse(Some(b"snappy")), None);
        assert!(Encoding::accepts_gzip(Some(b"identity, gzip")));
        assert!(!Encoding::accepts_gzip(Some(b"deflate")));
    }
}
