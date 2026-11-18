//! Answering "has this file changed since I fetched it?".
//!
//! A browser that has `site.css` in its cache sends back what identified
//! the copy it holds, and gets a short answer when nothing changed:
//!
//! ```text
//! GET /site.css                        200, ETag: "18a4f0c2d1e00-2b7"
//! GET /site.css
//! If-None-Match: "18a4f0c2d1e00-2b7"   304 Not Modified, no body
//! ```

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// What identifies one version of a file to a cache.
pub(crate) struct Validators {
    /// Quoted, as it appears in the `ETag` header.
    pub(crate) etag: String,
    /// In whole seconds: an HTTP date has no fractions.
    modified: SystemTime,
}

impl Validators {
    /// The validators of a file changed at `modified` and `length` bytes
    /// long. `variant` tells a precompressed copy (`-br`) from the file
    /// itself, since the two are different bytes.
    pub(crate) fn of(modified: SystemTime, length: u64, variant: &str) -> Validators {
        let since_epoch = modified.duration_since(UNIX_EPOCH).unwrap_or_default();
        Validators {
            etag: format!("\"{:x}-{length:x}{variant}\"", since_epoch.as_nanos()),
            modified: UNIX_EPOCH + Duration::from_secs(since_epoch.as_secs()),
        }
    }

    /// The validators of the same file packed another way (`-gzip`).
    pub(crate) fn of_variant(mut self, variant: &str) -> Validators {
        let closing_quote = self.etag.len() - 1;
        self.etag.insert_str(closing_quote, variant);
        self
    }

    /// The value of the `Last-Modified` header.
    pub(crate) fn last_modified(&self) -> String {
        httpdate::fmt_http_date(self.modified)
    }

    /// Whether the copy the client describes is still current, so that 304
    /// is the answer. `If-None-Match` decides alone when it is present.
    pub(crate) fn unchanged_for(
        &self,
        if_none_match: Option<&str>,
        if_modified_since: Option<&str>,
    ) -> bool {
        if let Some(tags) = if_none_match {
            return tags.trim() == "*"
                || tags
                    .split(',')
                    .any(|tag| without_weak_mark(tag) == self.etag);
        }
        if_modified_since
            .and_then(|date| httpdate::parse_http_date(date.trim()).ok())
            .is_some_and(|date| self.modified <= date)
    }

    /// Whether an `If-Range` value names exactly this version, so that the
    /// `Range` sent with it may be honoured. A weak tag never qualifies.
    pub(crate) fn is_named_by(&self, if_range: &str) -> bool {
        let if_range = if_range.trim();
        if if_range.starts_with('"') {
            return if_range == self.etag;
        }
        httpdate::parse_http_date(if_range).is_ok_and(|date| date == self.modified)
    }
}

/// `W/"abc"` and `"abc"` name the same file for the purpose of a 304.
fn without_weak_mark(tag: &str) -> &str {
    let tag = tag.trim();
    tag.strip_prefix("W/").unwrap_or(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validators() -> Validators {
        // 2026-11-18 08:00:00.5 UTC, 695 bytes.
        let modified = UNIX_EPOCH + Duration::from_millis(1_794_988_800_500);
        Validators::of(modified, 695, "")
    }

    #[test]
    fn the_tag_is_built_from_time_and_length() {
        assert_eq!(validators().etag, "\"18e914cdc59b6500-2b7\"");
        assert_eq!(
            Validators::of(UNIX_EPOCH, 0, "-br").etag,
            "\"0-0-br\"".to_string()
        );
        assert_eq!(
            validators().of_variant("-gzip").etag,
            "\"18e914cdc59b6500-2b7-gzip\""
        );
    }

    #[test]
    fn a_matching_tag_means_unchanged() {
        let current = validators();
        let tag = current.etag.clone();
        assert!(current.unchanged_for(Some(&tag), None));
        assert!(current.unchanged_for(Some(&format!("\"other\", W/{tag}")), None));
        assert!(current.unchanged_for(Some("*"), None));
        assert!(!current.unchanged_for(Some("\"other\""), None));
        // The tag decides alone, whatever the date says.
        let later = "Thu, 19 Nov 2026 08:00:00 GMT";
        assert!(!current.unchanged_for(Some("\"other\""), Some(later)));
    }

    #[test]
    fn a_date_at_or_after_the_change_means_unchanged() {
        let current = validators();
        assert_eq!(current.last_modified(), "Wed, 18 Nov 2026 08:00:00 GMT");
        assert!(current.unchanged_for(None, Some("Wed, 18 Nov 2026 08:00:00 GMT")));
        assert!(current.unchanged_for(None, Some("Thu, 19 Nov 2026 08:00:00 GMT")));
        assert!(!current.unchanged_for(None, Some("Wed, 18 Nov 2026 07:59:59 GMT")));
        assert!(!current.unchanged_for(None, Some("yesterday")));
        assert!(!current.unchanged_for(None, None));
    }

    #[test]
    fn if_range_needs_the_exact_version() {
        let current = validators();
        let tag = current.etag.clone();
        assert!(current.is_named_by(&tag));
        assert!(!current.is_named_by(&format!("W/{tag}")));
        assert!(current.is_named_by("Wed, 18 Nov 2026 08:00:00 GMT"));
        assert!(!current.is_named_by("Thu, 19 Nov 2026 08:00:00 GMT"));
    }
}
