//! PHP reads multipart bodies from the SAPI in windows. After a failed parse,
//! `file_get_contents('php://input')` can still read bytes beyond that window.

use std::sync::atomic::{AtomicBool, Ordering};

use memchr::memmem::Finder;

const FILL_UNIT: usize = 5120;

pub(super) struct Buffer<'a, 'cancel> {
    body: &'a [u8],
    cancelled: &'cancel AtomicBool,
    position: usize,
    buffered_end: usize,
}

impl<'a, 'cancel> Buffer<'a, 'cancel> {
    pub(super) fn new(body: &'a [u8], cancelled: &'cancel AtomicBool) -> Self {
        Self {
            body,
            cancelled,
            position: 0,
            buffered_end: 0,
        }
    }

    pub(super) fn consumed(&self) -> usize {
        self.buffered_end
    }

    fn fill(&mut self) {
        self.buffered_end = (self.position + FILL_UNIT).min(self.body.len());
    }

    pub(super) fn line(&mut self) -> Option<&'a [u8]> {
        if self.cancelled.load(Ordering::Relaxed) {
            return None;
        }
        self.next_line().or_else(|| {
            self.fill();
            self.next_line()
        })
    }

    fn next_line(&mut self) -> Option<&'a [u8]> {
        let available = &self.body[self.position..self.buffered_end];
        if let Some(newline) = memchr::memchr(b'\n', available) {
            self.position += newline + 1;
            let line = &available[..newline];
            return Some(line.strip_suffix(b"\r").unwrap_or(line));
        }
        if available.len() == FILL_UNIT {
            self.position += available.len();
            return Some(available);
        }
        None
    }

    /// Read up to the next delimiter. A matching prefix at the end of the
    /// window is held for the next refill so it does not become file data.
    pub(super) fn content(&mut self, delimiter: &[u8], limit: Option<usize>) -> (&'a [u8], bool) {
        let start = self.position;
        let mut end = start;
        let mut complete = false;
        let finder = Finder::new(delimiter);
        while !self.cancelled.load(Ordering::Relaxed) {
            if self.buffered_end - self.position < FILL_UNIT {
                self.fill();
            }
            let available = &self.body[self.position..self.buffered_end];
            let boundary = boundary_at(available, delimiter, &finder);
            complete |= boundary.is_some_and(|index| available[index..].starts_with(delimiter));
            let length = boundary.unwrap_or(available.len()).min(FILL_UNIT - 1);
            if length == 0 {
                break;
            }
            let trim_cr = boundary.is_some() && available[length - 1] == b'\r';
            let length = length - usize::from(trim_cr);
            if length == 0 {
                break;
            }
            self.position += length;
            end = self.position;
            if limit.is_some_and(|limit| end - start > limit) {
                break;
            }
        }
        (&self.body[start..end], complete)
    }
}

fn boundary_at(available: &[u8], delimiter: &[u8], finder: &Finder<'_>) -> Option<usize> {
    if let Some(index) = finder.find(available) {
        return Some(index);
    }
    // A partial match can only start in the last delimiter-length bytes.
    let tail = available
        .len()
        .saturating_sub(delimiter.len().saturating_sub(1));
    memchr::memchr_iter(b'\n', &available[tail..])
        .map(|index| tail + index)
        .find(|&index| delimiter.starts_with(&available[index..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn large_content_with_near_matches_uses_the_longest_boundary() {
        let mut boundary = [b'a'; 70];
        boundary[69] = b'b';
        let delimiter = [b"\n--", &boundary[..]].concat();
        let mut near_match = delimiter.clone();
        near_match[72] = b'c';
        let mut body = near_match.repeat((1024_usize * 1024).div_ceil(near_match.len()));
        let length = body.len();
        body.extend_from_slice(&delimiter);
        let cancelled = AtomicBool::new(false);
        let mut buffer = Buffer::new(&body, &cancelled);
        let start = Instant::now();
        let (content, complete) = buffer.content(&delimiter, None);
        assert!(complete);
        assert_eq!(content.len(), length);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
