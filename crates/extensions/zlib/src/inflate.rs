//! Incremental framing records the exact input consumed by an inflate context.

use flate2::{Decompress, FlushDecompress, Status};

use crate::deflate::Flush;
use crate::format::{self, Checksum, Encoding};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    Data,
    Memory,
    NeedDictionary,
    WrongDictionary,
}

impl Failure {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Data => "data error",
            Self::Memory => "insufficient memory",
            Self::NeedDictionary => "need dictionary",
            Self::WrongDictionary => "data error",
        }
    }
}

enum Phase {
    Header(Vec<u8>),
    Body,
    Trailer { bytes: [u8; 8], used: usize },
    Complete,
}

pub(crate) struct Decoder {
    codec: Decompress,
    encoding: Encoding,
    phase: Phase,
    checksum: Checksum,
    pub(crate) read_len: usize,
    pub(crate) status: i64,
    window: u8,
    dictionary: Option<Vec<u8>>,
}

impl Decoder {
    /// A fresh decoder with the same settings, for a fork of a snapshot,
    /// while nothing was decompressed yet; `None` once it started.
    pub(crate) fn unstarted_copy(&self) -> Option<Self> {
        (self.codec.total_in() == 0 && self.read_len == 0 && self.status == 0)
            .then(|| Self::new(self.encoding, self.window, self.dictionary.clone()))
    }

    pub(crate) fn new(encoding: Encoding, window: u8, dictionary: Option<Vec<u8>>) -> Self {
        let mut this = Self {
            codec: Decompress::new(false),
            encoding,
            phase: match encoding {
                Encoding::Raw => Phase::Body,
                _ => Phase::Header(Vec::new()),
            },
            checksum: Checksum::new(encoding),
            read_len: 0,
            status: 0,
            window,
            dictionary,
        };
        if encoding == Encoding::Raw
            && let Some(dictionary) = this.dictionary.take()
        {
            this.seed(&dictionary);
        }
        this
    }

    fn seed(&mut self, dictionary: &[u8]) {
        let dictionary = &dictionary[dictionary.len().saturating_sub(1 << self.window)..];
        let mut block = Vec::new();
        format::stored(&mut block, dictionary, false);
        let mut output = vec![0; dictionary.len() + 1];
        // A non-final stored block supplies the same initial history as a preset dictionary.
        let _ = self
            .codec
            .decompress(&block, &mut output, FlushDecompress::None);
    }

    fn header(&mut self, bytes: &[u8]) -> Result<Option<usize>, Failure> {
        if self.encoding == Encoding::Gzip {
            return format::gzip_header(bytes).map_err(|_| Failure::Data);
        }
        let Some(header) = bytes.first_chunk::<2>() else {
            return Ok(None);
        };
        if header[0] & 15 != 8
            || header[0] >> 4 > self.window - 8
            || !u16::from_be_bytes(*header).is_multiple_of(31)
        {
            return Err(Failure::Data);
        }
        if header[1] & 32 == 0 {
            return Ok(Some(2));
        }
        let Some(check) = bytes.get(2..6).and_then(|bytes| bytes.first_chunk::<4>()) else {
            return Ok(None);
        };
        let Some(dictionary) = self.dictionary.take() else {
            return Err(Failure::NeedDictionary);
        };
        if format::adler32(&dictionary) != u32::from_be_bytes(*check) {
            return Err(Failure::WrongDictionary);
        }
        self.seed(&dictionary);
        Ok(Some(6))
    }

    pub(crate) fn add(&mut self, input: &[u8], flush: Flush) -> Result<Vec<u8>, Failure> {
        if self.status == 1 {
            *self = Self::new(self.encoding, self.window, self.dictionary.take());
        }
        if input.is_empty() && flush != Flush::Finish {
            return Ok(Vec::new());
        }
        let mut output = Vec::new();
        let mut offset = 0;
        loop {
            let mut buffer = [0; 8192];
            let result = self.feed(&input[offset..], &mut buffer);
            let (used, wrote) = match result {
                Ok(amounts) => amounts,
                Err(failure) => {
                    self.status = match failure {
                        Failure::NeedDictionary | Failure::WrongDictionary => 2,
                        Failure::Memory => -4,
                        Failure::Data => -3,
                    };
                    return Err(failure);
                }
            };
            offset += used;
            output.try_reserve(wrote).map_err(|_| Failure::Memory)?;
            output.extend_from_slice(&buffer[..wrote]);
            if self.status == 1 {
                return Ok(output);
            }
            if wrote < buffer.len() && offset == input.len() || used == 0 && wrote == 0 {
                self.status = if flush == Flush::Finish || used == 0 && wrote == 0 {
                    -5
                } else {
                    0
                };
                return Ok(output);
            }
        }
    }

    pub(crate) fn feed(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<(usize, usize), Failure> {
        let mut used = 0;
        let mut wrote = 0;
        if matches!(self.phase, Phase::Header(_)) {
            used = self.consume_header(input)?;
            if matches!(self.phase, Phase::Header(_)) {
                return Ok((used, 0));
            }
        }
        if matches!(self.phase, Phase::Body) {
            let before_in = self.codec.total_in();
            let before_out = self.codec.total_out();
            let status = self
                .codec
                .decompress(&input[used..], output, FlushDecompress::None);
            let consumed = (self.codec.total_in() - before_in) as usize;
            self.read_len += consumed;
            used += consumed;
            wrote = (self.codec.total_out() - before_out) as usize;
            self.checksum.update(&output[..wrote]);
            if status.map_err(|_| Failure::Data)? == Status::StreamEnd {
                if self.encoding == Encoding::Raw {
                    self.phase = Phase::Complete;
                    self.status = 1;
                } else {
                    self.phase = Phase::Trailer {
                        bytes: [0; 8],
                        used: 0,
                    };
                }
            }
        }
        if let Phase::Trailer { bytes, used: seen } = &mut self.phase {
            let (wanted, length) = self.checksum.trailer();
            let count = (length - *seen).min(input.len() - used);
            bytes[*seen..*seen + count].copy_from_slice(&input[used..used + count]);
            *seen += count;
            used += count;
            self.read_len += count;
            if *seen == length {
                if bytes[..length] != wanted[..length] {
                    return Err(Failure::Data);
                }
                self.phase = Phase::Complete;
                self.status = 1;
            }
        }
        Ok((used, wrote))
    }

    fn consume_header(&mut self, input: &[u8]) -> Result<usize, Failure> {
        let Phase::Header(mut pending) = std::mem::replace(&mut self.phase, Phase::Body) else {
            return Ok(0);
        };
        let previous = pending.len();
        let bytes = if pending.is_empty() {
            input
        } else {
            pending
                .try_reserve(input.len())
                .map_err(|_| Failure::Memory)?;
            pending.extend_from_slice(input);
            &pending
        };
        let size = match self.header(bytes) {
            Ok(Some(length)) => length - previous,
            Ok(None) => {
                if pending.is_empty() {
                    pending
                        .try_reserve(input.len())
                        .map_err(|_| Failure::Memory)?;
                    pending.extend_from_slice(input);
                }
                self.phase = Phase::Header(pending);
                input.len()
            }
            Err(failure) => {
                let unread: usize =
                    if matches!(failure, Failure::NeedDictionary | Failure::WrongDictionary) {
                        6
                    } else {
                        2
                    };
                self.read_len += unread.saturating_sub(previous).min(input.len());
                return Err(failure);
            }
        };
        self.read_len += size;
        Ok(size)
    }
}
