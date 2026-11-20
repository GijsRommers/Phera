//! `deflate_add($ctx, $data, ZLIB_FINISH)` finishes and resets one member.

use flate2::{Compress, Compression, FlushCompress, Status};

use crate::format::{self, Checksum, Encoding};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flush {
    None,
    Partial,
    Sync,
    Full,
    Finish,
    Block,
}

impl Flush {
    pub(crate) fn from_php(value: i64) -> Option<Self> {
        Some(match value {
            0 => Self::None,
            1 => Self::Partial,
            2 => Self::Sync,
            3 => Self::Full,
            4 => Self::Finish,
            5 => Self::Block,
            _ => return None,
        })
    }
}

pub(crate) struct Encoder {
    codec: Compress,
    encoding: Encoding,
    checksum: Checksum,
    level: u32,
    window: u8,
    dictionary: Option<Vec<u8>>,
    started: bool,
}

impl Encoder {
    pub(crate) fn new(
        encoding: Encoding,
        level: u32,
        window: u8,
        dictionary: Option<Vec<u8>>,
    ) -> Self {
        Self {
            codec: Compress::new(Compression::new(level), false),
            encoding,
            checksum: Checksum::new(encoding),
            level,
            window,
            dictionary: dictionary.filter(|bytes| !bytes.is_empty()),
            started: false,
        }
    }

    pub(crate) fn add(&mut self, input: &[u8], flush: Flush) -> Result<Vec<u8>, ()> {
        if input.is_empty() && flush != Flush::Finish {
            return Ok(Vec::new());
        }
        let mut output = Vec::new();
        if !self.started {
            self.encoding.header(
                &mut output,
                self.level,
                self.window,
                self.dictionary.as_deref(),
            );
            self.started = true;
        }
        self.checksum.update(input);
        let finished = if self.level == 0 && self.codec.total_in() == 0 && flush == Flush::Finish {
            format::stored(&mut output, input, true);
            true
        } else {
            self.compress(input, &mut output, flush)?
        };
        if finished {
            let (trailer, length) = self.checksum.trailer();
            output.extend(&trailer[..length]);
            self.codec.reset();
            self.checksum = Checksum::new(self.encoding);
            self.dictionary = None;
            self.started = false;
        }
        Ok(output)
    }

    fn compress(&mut self, input: &[u8], output: &mut Vec<u8>, flush: Flush) -> Result<bool, ()> {
        let flush = match flush {
            Flush::None => FlushCompress::None,
            Flush::Partial => FlushCompress::Partial,
            Flush::Sync | Flush::Block => FlushCompress::Sync,
            Flush::Full => FlushCompress::Full,
            Flush::Finish => FlushCompress::Finish,
        };
        let mut chunk = [0; 8192];
        let mut offset = 0;
        loop {
            let before_in = self.codec.total_in();
            let before_out = self.codec.total_out();
            let status = self
                .codec
                .compress(&input[offset..], &mut chunk, flush)
                .map_err(|_| ())?;
            let used = (self.codec.total_in() - before_in) as usize;
            let wrote = (self.codec.total_out() - before_out) as usize;
            offset += used;
            output.try_reserve(wrote).map_err(|_| ())?;
            output.extend_from_slice(&chunk[..wrote]);
            if status == Status::StreamEnd {
                return Ok(true);
            }
            if used == 0 && wrote == 0 {
                return if flush == FlushCompress::Finish {
                    Err(())
                } else {
                    Ok(false)
                };
            }
            if offset == input.len() && wrote < chunk.len() && flush != FlushCompress::Finish {
                return Ok(false);
            }
        }
    }
}
