//! Finding the matches of a pattern one after another.
//!
//! ```php
//! preg_match_all('/(?:|a)/', 'a', $m);   // 3: "" at 0, then "a", then "" at 1
//! ```
//!
//! After an empty match PHP tries again at the same place, this time for a
//! match that is not empty and starts right there; only when that fails
//! does it move one character on. The safe pcre2 API takes no options per
//! call, so the retry runs a second compiled pattern that says the same
//! with `(*NOTEMPTY_ATSTART)` and `\G` (see `Compiled::retry`).

use crate::compile::Compiled;
use crate::errors::Error;
use pcre2::bytes::CaptureLocations;

/// The span of each group of one match, group 0 being the whole match;
/// `None` for a group that did not take part.
pub(crate) type Captures = Vec<Option<(usize, usize)>>;

/// The matches of `compiled` in `subject` from `position` on, as an
/// iterator. An error ends it.
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

impl Iterator for Scanner<'_> {
    type Item = Result<Captures, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let found = self.find();
        if found.is_err() {
            self.finished = true;
        }
        found.transpose()
    }
}

impl<'a> Scanner<'a> {
    /// `position` must be at most `subject.len()`; `preg_match` checks its
    /// offset before it gets here.
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

    /// With `/u`, the subject from `position` on must be UTF-8, and
    /// `position` must not fall inside a character.
    fn validate(&self) -> Result<(), Error> {
        if self.compiled.utf {
            if self
                .subject
                .get(self.position)
                .is_some_and(|&byte| is_continuation(byte))
            {
                return Err(Error::Utf8Offset);
            }
            if std::str::from_utf8(&self.subject[self.position..]).is_err() {
                return Err(Error::Utf8);
            }
        }
        Ok(())
    }

    /// Runs the retry that would follow an empty match, for its error only.
    /// `preg_match` without `$matches` does this in PHP: a backtrack limit
    /// hit there makes it return false.
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

    fn find(&mut self) -> Result<Option<Captures>, Error> {
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
            if let Some(found) = regex.captures_read_at(locations, self.subject, self.position)? {
                // \K in a lookahead can end a match before its start; PHP
                // reports that as an internal error.
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
            // The retry failed: move one character on and search normally.
            self.position += 1;
            if self.compiled.utf {
                while self
                    .subject
                    .get(self.position)
                    .is_some_and(|&byte| is_continuation(byte))
                {
                    self.position += 1;
                }
            }
            self.after_empty = false;
        }
    }
}

/// A byte in the middle of a UTF-8 character: `10xxxxxx`.
fn is_continuation(byte: u8) -> bool {
    byte & 0xc0 == 0x80
}
