//! Global matching retries after empties: `preg_match_all('/(?:|a)/', 'a', $m)` is 3.

use crate::compile::Compiled;
use crate::errors::Error;

pub(crate) type Captures = Vec<Option<(usize, usize)>>;
pub(crate) struct Scanner<'a> {
    compiled: &'a Compiled,
    subject: &'a [u8],
    position: usize,
    after_empty: bool,
    finished: bool,
    validated: bool,
}
impl<'a> Scanner<'a> {
    pub(crate) fn new(compiled: &'a Compiled, subject: &'a [u8], position: usize) -> Self {
        Self {
            compiled,
            subject,
            position,
            after_empty: false,
            finished: false,
            validated: false,
        }
    }
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.position > self.subject.len() {
            return Err(Error::Internal);
        }
        if self.compiled.utf {
            if self.position < self.subject.len() && self.subject[self.position] & 0xc0 == 0x80 {
                return Err(Error::Utf8Offset);
            }
            if std::str::from_utf8(&self.subject[self.position..]).is_err() {
                return Err(Error::Utf8);
            }
        }
        Ok(())
    }
    /// PHP performs this retry even for preg_match when no $matches argument was supplied.
    pub(crate) fn check_empty_retry(&self) -> Result<(), Error> {
        if self.after_empty {
            let retry = self.compiled.retry()?;
            let mut locations = retry.capture_locations();
            retry.captures_read_at(&mut locations, self.subject, self.position)?;
        }
        Ok(())
    }
    pub(crate) fn next(&mut self) -> Result<Option<Captures>, Error> {
        if self.finished {
            return Ok(None);
        }
        if !self.validated {
            self.validate()?;
            self.validated = true;
        }
        loop {
            let regex = if self.after_empty {
                self.compiled.retry()?
            } else {
                &self.compiled.regex
            };
            let mut locations = regex.capture_locations();
            let found = regex.captures_read_at(&mut locations, self.subject, self.position)?;
            if let Some(found) = found {
                if found.end() < found.start() {
                    return Err(Error::Internal);
                }
                self.position = found.end();
                self.after_empty = found.start() == found.end();
                return Ok(Some(
                    (0..locations.len()).map(|i| locations.get(i)).collect(),
                ));
            }
            if !self.after_empty || self.position == self.subject.len() {
                self.finished = true;
                return Ok(None);
            }
            self.position += 1;
            if self.compiled.utf {
                while self.position < self.subject.len()
                    && self.subject[self.position] & 0xc0 == 0x80
                {
                    self.position += 1;
                }
            }
            self.after_empty = false;
        }
    }
}
