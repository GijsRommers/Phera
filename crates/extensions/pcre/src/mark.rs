//! Recover the MARK for a successful match without changing its captures.
//!
//! ```php
//! preg_match('/(?:a(*:x)|b(*:y))+/', 'ba', $m);
//! // $m['MARK'] === 'x'
//! ```

use crate::mark_pattern::Probe;
use pcre2::bytes::{CaptureLocations, Regex, RegexBuilder};
use std::cell::{OnceCell, RefCell};

struct MarkRegex {
    regex: Regex,
    locations: RefCell<CaptureLocations>,
}

pub(crate) struct Marks {
    builder: RegexBuilder,
    source: String,
    retry_source: String,
    normal: OnceCell<Option<MarkRegex>>,
    retry: OnceCell<Option<MarkRegex>>,
    probes: Vec<(usize, Vec<u8>)>,
    groups: usize,
}

impl Marks {
    pub(crate) fn new(
        builder: RegexBuilder,
        probe: Probe,
        prefix: &str,
        closing: &str,
        anchored: bool,
    ) -> Self {
        let body = probe.body;
        let anchored_source = format!("{prefix}(?(R)|\\G)(?:{body}{closing})");
        let retry_source = format!("(*NOTEMPTY_ATSTART){anchored_source}");
        let source = if anchored {
            anchored_source
        } else {
            format!("{prefix}{body}")
        };
        Self {
            builder,
            source,
            retry_source,
            normal: OnceCell::new(),
            retry: OnceCell::new(),
            probes: probe.marks,
            groups: probe.groups,
        }
    }

    /// These regexes are compiled only for callers that expose match arrays.
    pub(crate) fn find(
        &self,
        subject: &[u8],
        position: usize,
        retry: bool,
        span: (usize, usize),
    ) -> Option<&[u8]> {
        let (cell, source) = if retry {
            (&self.retry, &self.retry_source)
        } else {
            (&self.normal, &self.source)
        };
        let compiled = cell
            .get_or_init(|| {
                let regex = self.builder.build(source).ok()?;
                if regex.captures_len() != self.groups + 1 {
                    return None;
                }
                let locations = RefCell::new(regex.capture_locations());
                Some(MarkRegex { regex, locations })
            })
            .as_ref()?;
        let mut captures = compiled.locations.borrow_mut();
        let whole = compiled
            .regex
            .captures_read_at(&mut captures, subject, position)
            .ok()??;
        if (whole.start(), whole.end()) != span {
            return None;
        }
        // Take the probe furthest into the subject. At the same offset,
        // max_by_key keeps the last probe in pattern order.
        self.probes
            .iter()
            .filter_map(|(group, name)| Some((captures.get(*group)?.0, name.as_slice())))
            .max_by_key(|(offset, _)| *offset)
            .map(|(_, name)| name)
    }
}
