//! PHP reads multipart bodies from the SAPI in windows. After a failed parse,
//! `file_get_contents('php://input')` can still read bytes beyond that window.

pub(super) const FILL_UNIT: usize = 5120;

pub(super) struct Buffer<'a> {
    body: &'a [u8],
    capacity: usize,
    position: usize,
    buffered_end: usize,
}

impl<'a> Buffer<'a> {
    pub(super) fn new(body: &'a [u8], boundary_length: usize) -> Self {
        Self {
            body,
            capacity: FILL_UNIT.max(boundary_length + 6),
            position: 0,
            buffered_end: 0,
        }
    }

    pub(super) fn consumed(&self) -> usize {
        self.buffered_end
    }

    fn fill(&mut self) {
        self.buffered_end = (self.position + self.capacity).min(self.body.len());
    }

    pub(super) fn line(&mut self) -> Option<&'a [u8]> {
        self.next_line().or_else(|| {
            self.fill();
            self.next_line()
        })
    }

    fn next_line(&mut self) -> Option<&'a [u8]> {
        let available = &self.body[self.position..self.buffered_end];
        if let Some(newline) = available.iter().position(|&byte| byte == b'\n') {
            self.position += newline + 1;
            let line = &available[..newline];
            return Some(line.strip_suffix(b"\r").unwrap_or(line));
        }
        if available.len() == self.capacity {
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
        loop {
            if self.buffered_end - self.position < FILL_UNIT {
                self.fill();
            }
            let available = &self.body[self.position..self.buffered_end];
            let boundary = boundary_at(available, delimiter);
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

fn boundary_at(available: &[u8], delimiter: &[u8]) -> Option<usize> {
    available.iter().enumerate().find_map(|(index, &byte)| {
        if byte != b'\n' {
            return None;
        }
        let remainder = &available[index..];
        let length = remainder.len().min(delimiter.len());
        (remainder[..length] == delimiter[..length]).then_some(index)
    })
}
