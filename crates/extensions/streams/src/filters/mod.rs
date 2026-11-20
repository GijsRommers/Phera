//! `stream_filter_append($file, 'string.rot13')` transforms each bucket on its way through.

mod codec;
mod compression;
mod dechunk;
mod functions;
mod quoted;
mod uri;
pub(crate) mod user;

pub(crate) use functions::register;
pub(crate) use uri::FilterUri;

use std::collections::VecDeque;
use std::io;

use phera_extension_api::Value;

use codec::Codec;

pub(crate) const MAX_STAGES: usize = 16;
pub(crate) const MAX_BUCKET: usize = 8 * 1024 * 1024;
const MAX_OUTPUT: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Read,
    Write,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flush {
    No,
    Incremental,
    Closing,
}

pub(crate) struct Filter {
    id: i64,
    codec: Codec,
}

impl Filter {
    pub fn new(id: i64, name: &[u8], params: &Value) -> Option<Self> {
        Codec::new(name, params).map(|codec| Self { id, codec })
    }
}

#[derive(Default)]
pub(crate) struct Filters {
    pub read: Chain,
    pub write: Chain,
    pub buffered: VecDeque<u8>,
    pub read_closed: bool,
}

#[derive(Default)]
pub(crate) struct Chain {
    stages: VecDeque<Filter>,
    output: usize,
    failed: bool,
    seek_to: Option<u64>,
}

impl Chain {
    pub fn active(&self) -> bool {
        !self.stages.is_empty() || self.failed
    }

    pub fn len(&self) -> usize {
        self.stages.len()
    }

    pub fn add(&mut self, filter: Filter, prepend: bool) {
        if prepend {
            self.stages.push_front(filter);
        } else {
            self.stages.push_back(filter);
        }
    }

    pub fn remove(&mut self, id: i64, position: u64) -> io::Result<Option<Vec<u8>>> {
        let Some(index) = self.stages.iter().position(|stage| stage.id == id) else {
            return Ok(None);
        };
        self.seek_to = self.stages[index]
            .codec
            .seek_after(0, Flush::Closing, position);
        let mut data = self.stages[index].codec.apply(Vec::new(), Flush::Closing)?;
        self.stages.remove(index);
        for filter in self.stages.iter_mut().skip(index) {
            filter.codec.seek_after(data.len(), Flush::No, position);
            data = filter.codec.apply(data, Flush::No)?;
        }
        self.account(data.len())?;
        Ok(Some(data))
    }

    pub fn run(&mut self, mut data: Vec<u8>, flush: Flush, position: u64) -> io::Result<Vec<u8>> {
        if self.failed {
            return Err(limit());
        }
        let result = (|| {
            bounded(data.len())?;
            for filter in &mut self.stages {
                if let Some(position) = filter.codec.seek_after(data.len(), flush, position) {
                    self.seek_to = Some(position);
                }
                data = filter.codec.apply(data, flush)?;
                bounded(data.len())?;
            }
            self.account(data.len())?;
            Ok(data)
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    pub fn take_seek(&mut self) -> Option<u64> {
        self.seek_to.take()
    }

    fn account(&mut self, size: usize) -> io::Result<()> {
        self.output = self.output.saturating_add(size);
        if self.output > MAX_OUTPUT {
            return Err(limit());
        }
        Ok(())
    }
}

pub(crate) fn bounded(size: usize) -> io::Result<()> {
    if size > MAX_BUCKET {
        Err(limit())
    } else {
        Ok(())
    }
}

pub(super) fn limit() -> io::Error {
    io::Error::other("Stream filter output exceeds Phera's size limit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_refuses_excessive_expansion_before_returning_output() {
        use phera_extension_api::{ArrayKey, PhpArray};
        let mut options = PhpArray::new();
        options.insert(ArrayKey::str("line-length"), Value::Int(4));
        options.insert(
            ArrayKey::str("line-break-chars"),
            Value::from(vec![b'_'; MAX_BUCKET]),
        );
        let mut chain = Chain::default();
        let filter = Filter::new(1, b"convert.base64-encode", &Value::from(options)).unwrap();
        chain.add(filter, false);
        assert!(chain.run(b"abcdef".to_vec(), Flush::No, 0).is_err());
        assert!(chain.run(Vec::new(), Flush::Closing, 0).is_err());
    }
}
