//! `file_get_contents('php://input')` buffers bytes read from the SAPI.
//! A form parser reads the remaining SAPI bytes, rather than the buffered copy.

use std::io::{self, SeekFrom};
use std::ops::Range;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

#[derive(Clone, Default)]
pub struct RequestInput {
    bytes: Arc<[u8]>,
    state: Arc<Mutex<Incoming>>,
}

#[derive(Default)]
struct Incoming {
    consumed: usize,
    post_read: bool,
    raw: Arc<Mutex<Buffered>>,
}

#[derive(Default)]
struct Buffered {
    ranges: Vec<Range<usize>>,
    length: usize,
}

/// A PHP input stream keeps its own cursor and the buffer in use when it opened.
pub(crate) struct InputStream {
    request: RequestInput,
    raw: Arc<Mutex<Buffered>>,
    position: u64,
}

impl RequestInput {
    pub(crate) fn new(bytes: Arc<[u8]>) -> Self {
        Self {
            bytes,
            state: Arc::default(),
        }
    }

    /// The automatic urlencoded POST reader has already buffered the input.
    pub fn mark_post_read(&self) {
        self.read_urlencoded();
    }

    /// The standard POST reader replaces the raw stream with what it reads.
    pub fn read_urlencoded(&self) -> Range<usize> {
        let mut state = lock(&self.state);
        let range = state.consumed..self.bytes.len();
        state.consumed = range.end;
        state.post_read = true;
        let mut raw = Buffered::default();
        raw.append(range.clone());
        state.raw = Arc::new(Mutex::new(raw));
        range
    }

    /// Read multipart data directly, without caching it in `php://input`.
    pub fn read_multipart(&self) -> Range<usize> {
        let mut state = lock(&self.state);
        let range = state.consumed..self.bytes.len();
        state.consumed = range.end;
        range
    }

    pub fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }

    pub(crate) fn open(&self) -> InputStream {
        InputStream {
            request: self.clone(),
            raw: lock(&self.state).raw.clone(),
            position: 0,
        }
    }
}

impl InputStream {
    pub(crate) fn size_hint(&self) -> usize {
        self.request.bytes.len()
    }

    pub(crate) fn read(&mut self, target: &mut [u8], chunk: usize) -> usize {
        let mut written = 0;
        self.read_with(target.len(), chunk, |bytes| {
            target[written..written + bytes.len()].copy_from_slice(bytes);
            written += bytes.len();
            bytes.len()
        });
        written
    }

    pub(crate) fn line(&mut self, limit: usize, chunk: usize) -> Vec<u8> {
        let mut line = Vec::new();
        while line.len() < limit {
            let read = self.read_with((limit - line.len()).min(chunk), chunk, |bytes| {
                let length = bytes
                    .iter()
                    .position(|&byte| byte == b'\n')
                    .map_or(bytes.len(), |newline| newline + 1);
                line.extend_from_slice(&bytes[..length]);
                length
            });
            if read == 0 || line.ends_with(b"\n") {
                break;
            }
        }
        line
    }

    fn read_with(
        &mut self,
        length: usize,
        chunk: usize,
        mut copy: impl FnMut(&[u8]) -> usize,
    ) -> usize {
        let mut state = lock(&self.request.state);
        let mut raw = lock(&self.raw);
        let position = usize::try_from(self.position).unwrap_or(usize::MAX);
        let end = position.saturating_add(length);
        if end > raw.length && !state.post_read && Arc::ptr_eq(&self.raw, &state.raw) {
            let start = state.consumed;
            state.consumed = start
                .saturating_add(length.max(chunk))
                .min(self.size_hint());
            raw.append(start..state.consumed);
        }

        let mut offset = 0;
        let mut read = 0;
        for range in &raw.ranges {
            let next = offset + range.len();
            let start = position.max(offset);
            let stop = end.min(next);
            if start < stop {
                let bytes =
                    &self.request.bytes[range.start + start - offset..range.start + stop - offset];
                let copied = copy(bytes);
                read += copied;
                if copied < bytes.len() || stop == end {
                    break;
                }
            }
            offset = next;
        }
        self.position += read as u64;
        read
    }

    pub(crate) fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let position = match position {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            SeekFrom::End(offset) => (lock(&self.raw).length as u64).checked_add_signed(offset),
        };
        self.position = position.ok_or(io::ErrorKind::InvalidInput)?;
        Ok(self.position)
    }
}

impl Buffered {
    fn append(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        self.length += range.len();
        if let Some(last) = self.ranges.last_mut()
            && last.end == range.start
        {
            last.end = range.end;
        } else {
            self.ranges.push(range);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
