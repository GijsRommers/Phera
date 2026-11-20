//! `zlib.inflate` and `zlib.deflate` retain the inflater or deflater between buckets.

use std::io;

use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};

use super::{Flush, bounded};

pub(super) enum Zlib {
    Inflate(Decompress),
    Deflate(Compress),
}

impl Zlib {
    pub fn inflate(wrapped: bool) -> Self {
        Self::Inflate(Decompress::new(wrapped))
    }

    pub fn deflate(level: u32, wrapped: bool) -> Self {
        Self::Deflate(Compress::new(Compression::new(level), wrapped))
    }

    pub fn run(&mut self, mut bytes: &[u8], flush: Flush) -> io::Result<Vec<u8>> {
        let mut buffer = [0; crate::io_policy::CHUNK];
        let mut result = Vec::new();
        loop {
            let (status, consumed, written) = self.step(bytes, &mut buffer, flush)?;
            bounded(result.len() + written)?;
            result.extend_from_slice(&buffer[..written]);
            bytes = &bytes[consumed..];
            if status == Status::StreamEnd || consumed == 0 && written == 0 {
                return Ok(result);
            }
            if bytes.is_empty() && written < buffer.len() {
                return Ok(result);
            }
        }
    }

    fn step(
        &mut self,
        input: &[u8],
        output: &mut [u8],
        flush: Flush,
    ) -> io::Result<(Status, usize, usize)> {
        match self {
            Self::Inflate(stream) => {
                let read = stream.total_in();
                let written = stream.total_out();
                let flush = if flush == Flush::Closing {
                    FlushDecompress::Finish
                } else {
                    FlushDecompress::None
                };
                let status = stream.decompress(input, output, flush).map_err(invalid)?;
                Ok((
                    status,
                    (stream.total_in() - read) as usize,
                    (stream.total_out() - written) as usize,
                ))
            }
            Self::Deflate(stream) => {
                let read = stream.total_in();
                let written = stream.total_out();
                let flush = match flush {
                    Flush::No => FlushCompress::None,
                    Flush::Incremental => FlushCompress::Sync,
                    Flush::Closing => FlushCompress::Finish,
                };
                let status = stream.compress(input, output, flush).map_err(invalid)?;
                Ok((
                    status,
                    (stream.total_in() - read) as usize,
                    (stream.total_out() - written) as usize,
                ))
            }
        }
    }
}

fn invalid(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}
