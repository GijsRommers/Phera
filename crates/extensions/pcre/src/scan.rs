//! Global matching retries after empties: `preg_match_all('/(?:|a)/', 'a', $m)` is 3.

use crate::compile::Compiled;
use crate::errors::Error;
use pcre2::bytes::CaptureLocations;

pub(crate) type Captures = Vec<Option<(usize, usize)>>;
pub(crate) struct Scanner<'a> {
    compiled: &'a Compiled,
    subject: &'a [u8],
    position: usize,
    after_empty: bool,
    finished: bool,
    validated: bool,
    /// Borrowed from `compiled` for the scan and given back on drop; `None`
    /// only while being given back.
    locations: Option<CaptureLocations>,
    /// Match data for the retry pattern, made on the first retry.
    retry_locations: Option<CaptureLocations>,
}
impl Drop for Scanner<'_> {
    fn drop(&mut self) {
        if let Some(locations) = self.locations.take() {
            self.compiled.put_back_locations(locations);
        }
    }
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
            locations: Some(compiled.take_locations()),
            retry_locations: None,
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
    pub(crate) fn check_empty_retry(&mut self) -> Result<(), Error> {
        if self.after_empty {
            let retry = self.compiled.retry()?;
            let locations = self
                .retry_locations
                .get_or_insert_with(|| retry.capture_locations());
            retry.captures_read_at(locations, self.subject, self.position)?;
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
            let (regex, locations) = if self.after_empty {
                let retry = self.compiled.retry()?;
                let locations = self
                    .retry_locations
                    .get_or_insert_with(|| retry.capture_locations());
                (retry, locations)
            } else {
                let locations = self.locations.as_mut().expect("held until drop");
                (&self.compiled.regex, locations)
            };
            let found = regex.captures_read_at(locations, self.subject, self.position)?;
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
